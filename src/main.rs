use std::collections::HashMap;
use std::{env, thread};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
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
                    let mut response = if let Ok(req) = Request::new(&_stream) {
                        match req.path.iter().map(|s| s.as_str()).collect::<Vec<&str>>().as_slice() {
                            ["", ""] =>
                                Response::new(_stream),
                            ["", "user-agent"] =>
                                Response::new(_stream).with_header("Content-Type", "text/plain").with_content(req.headers.get("user-agent").unwrap_or(&"".to_string())),
                            ["", "files", filename] => {
                                let local_file = Path::new(serving_folder.as_str()).join(filename);
                                if let Ok(f) = File::open(&local_file) {
                                    let size = f.metadata().unwrap().len();
                                    let reader = BufReader::new(f);
                                    Response::new(_stream)
                                        .with_header("Content-Type", "application/octet-stream")
                                        .with_content_reader(reader, size)
                                } else {
                                    println!("not found {:?}", local_file);
                                    Response::new(_stream).with_code(404)
                                }
                            }
                            ["", "echo", rest @ .. ] =>
                                {
                                    let content = rest.join("/");
                                    Response::new(_stream).with_header("Content-Type", "text/plain")
                                        .with_content(&content)
                                }
                            _ =>
                                Response::new(_stream).with_code(404),
                        }
                    } else {
                        Response::new(_stream).with_code(400)
                    };
                    println!("response: {:?}", response);
                    response.flush().unwrap()
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        });
    }
}


#[derive(Debug)]
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
}

impl Request {
    pub fn new(stream: &TcpStream) -> Result<Self> {
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
        })
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
                            println!("read {} bytes", n);
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