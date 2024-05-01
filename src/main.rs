use std::collections::HashMap;
use std::{env, thread};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use itertools::Itertools;
use anyhow::{bail, Result};
use std::io::prelude::*;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let args_str = args
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>();
    println!("args: {:?}", args_str);
    let serving_folder = match args_str.as_slice() {
        ["--directory", directory] => directory,
        _ => ".",
    };


    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        let serving_folder = serving_folder.to_string();
        thread::spawn(move || {
            match stream {
                Ok(_stream) => {
                    if let Ok(mut req) = Request::new(_stream.try_clone().unwrap()) {
                        println!("accepted {:?}", req);
                        let mut response = match (req.method, req.path.iter().map(|s| s.as_str()).collect::<Vec<&str>>().as_slice()) {
                            (Method::GET, ["", ""]) =>
                                req.response(),
                            (Method::GET, ["", "user-agent"]) =>
                                {
                                    let user_agent = req.headers.get("user-agent").map(|s| {
                                        s.clone()
                                    }).unwrap_or("".to_string());
                                    req.response().with_header("Content-Type", "text/plain").with_content(&user_agent)
                                }
                            (Method::GET, ["", "files", filename]) => {
                                let local_file = Path::new(serving_folder.as_str()).join(filename);
                                if let Ok(f) = File::open(&local_file) {
                                    let size = f.metadata().unwrap().len();
                                    let reader = BufReader::new(f);
                                    req.response()
                                        .with_header("Content-Type", "application/octet-stream")
                                        .with_content_reader(reader, size)
                                } else {
                                    req.response().with_code(404)
                                }
                            }
                            (Method::POST, ["", "files", filename]) => {
                                let local_file = Path::new(serving_folder.as_str()).join(filename);
                                let content_length = req.headers.get("content-length").map(|s| {
                                    s.parse::<u32>()
                                }).unwrap().ok();

                                let f = File::create(local_file).unwrap();
                                let f = BufWriter::new(f);
                                req.write_content(f, content_length).unwrap();
                                req
                                    .response()
                                    .with_code(201)
                            }
                            (Method::GET, ["", "echo", rest @ .. ]) =>
                                {
                                    let content = rest.join("/");
                                    req.response().with_header("Content-Type", "text/plain")
                                        .with_content(&content)
                                }
                            _ =>
                                req.response().with_code(404),
                        };
                        response.flush().unwrap_or(());
                    } else {
                        println!("request failed to parse");
                    };
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}


#[derive(Debug, Copy, Clone)]
enum Method {
    GET,
    POST,
}

impl TryFrom<&str> for Method {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            _ => bail!("invalid method {}", value)
        }
    }
}

#[derive(Debug)]
struct Request {
    pub headers: HashMap<String, String>,
    pub method: Method,
    pub uri: String,
    pub path: Vec<String>,
    pub buf_reader: BufReader<TcpStream>,
}

impl Request {
    pub fn new(stream: TcpStream) -> Result<Self> {
        let mut buf_reader = BufReader::new(stream);

        let mut buffer = String::new();
        buf_reader.read_line(&mut buffer).unwrap_or(0);

        let mut lines = buffer.lines();
        let first_line = lines.next().unwrap();
        // read headline
        let mut parts = first_line.split(' ');
        let _method = parts.next().unwrap_or_default();
        let uri = parts.next().unwrap_or_default();
        let path = uri.split("/").map(|s| s.to_string()).collect::<Vec<String>>();
        let _protocol = parts.next().unwrap_or_default();

        // read headers
        let mut headers = HashMap::new();

        loop {
            let mut header = String::new();
            if let Ok(_header_size) = buf_reader.read_line(&mut header) {
                let header = header.trim();
                if header == "" {
                    break;
                }
                if let Some((key, value)) = header.split(":").map(|s| s.trim()).collect_tuple() {
                    headers.insert(key.to_lowercase().to_string(), value.to_string());
                } else {
                    continue;
                }
            }
        }

        Ok(Request {
            uri: uri.to_string(),
            method: _method.try_into()?,
            headers,
            path,
            buf_reader,
        })
    }
    pub fn write_content(&mut self, mut writer: BufWriter<File>, expected_len: Option<u32>) -> Result<()> {
        let content = &mut self.buf_reader;
        let mut buf = [0; 1024];
        let mut len = 0_u32;
        loop {
            if let Some(expected_len) = expected_len {
                if len >= expected_len {
                    break
                }
            }
            match content.read(&mut buf) {
                Ok(0) => {
                    break},
                Ok(n) => {
                    writer.write_all(&buf[..n])?;
                    len += n as u32;
                }
                Err(e) => bail!(e),
            }
        }
        Ok(())
    }

    pub fn response(self) -> Response {
        Response::new(self.buf_reader.into_inner())
    }
}

#[derive(Debug)]
enum Content {
    Text(String),
    Bytes(BufReader<File>, u64),
}

#[derive(Debug)]
struct Response {
    stream: TcpStream,
    code: u16,
    content: Content,
    pub headers: HashMap<String, String>,
}

impl Response {
    pub fn new(stream: TcpStream) -> Self {
        Response {
            stream,
            code: 200,
            content: Content::Text("".to_string()),
            headers: HashMap::new(),
        }
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    pub fn with_code(mut self, code: u16) -> Self {
        self.code = code;
        self
    }

    pub fn with_content(mut self, content: &str) -> Self {
        let content = content.to_string();
        self.content = Content::Text(content);
        self
    }

    pub fn with_content_reader(mut self, content: BufReader<File>, size: u64) -> Self {
        self.content = Content::Bytes(content, size);
        self
    }

    pub fn flush(&mut self) -> Result<()> {
        write!(self.stream, "HTTP/1.1 {} OK\r\n", self.code)?;
        for (k, v) in self.headers.iter() {
            write!(self.stream, "{}: {}\r\n", k, v)?;
        }
        match self.content {
            Content::Text(ref content) => {
                write!(self.stream, "Content-Length: {}\r\n", content.len())?;
                write!(self.stream, "\r\n")?;
                write!(self.stream, "{}", content)?;
            }
            Content::Bytes(ref mut content, size) => {
                write!(self.stream, "Content-Length: {}\r\n", size)?;
                write!(self.stream, "\r\n")?;
                // read from Read and write to Write
                let mut buf = [0; 1024];
                loop {
                    match content.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            self.stream.write_all(&buf[..n])?
                        }
                        Err(e) => bail!(e),
                    }
                }
            }
        }

        self.stream.flush()?;
        Ok(())
    }
}