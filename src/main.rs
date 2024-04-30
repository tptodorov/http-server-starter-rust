use std::collections::HashMap;
use std::{io, thread};
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use itertools::Itertools;
use anyhow::{bail, Result};

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        thread::spawn(|| {
            match stream {
                Ok(mut _stream) => {
                    let mut response = if let Ok(req) = Request::new(&_stream) {
                        match req.path.iter().map(|s| s.as_str()).collect::<Vec<&str>>().as_slice() {
                            ["", ""] =>
                                Response::new(_stream),
                            ["", "user-agent"] =>
                                Response::new(_stream).with_header("Content-Type", "text/plain").with_content(req.headers.get("user-agent").unwrap_or(&"".to_string())),
                            ["", "echo", rest @ .. ] =>
                                {
                                    let content = rest.join("/");
                                    Response::new(_stream).with_header("Content-Type", "text/plain").with_content(&content)
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
        let mut buf_reader = io::BufReader::new(stream);

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
struct Response {
    stream: TcpStream,
    code: u16,
    content: String,
    pub headers: HashMap<String, String>,
}

impl Response {
    pub fn new(stream: TcpStream) -> Self {
        Response {
            stream,
            code: 200,
            content: "".to_string(),
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
        self.content = content.to_string();
        self
    }

    pub fn flush(&mut self) -> Result<()> {
        write!(self.stream, "HTTP/1.1 {} OK\r\n", self.code)?;
        for (k, v) in self.headers.iter() {
            write!(self.stream, "{}: {}\r\n", k, v)?;
        }
        write!(self.stream, "Content-Length: {}\r\n", self.content.len())?;
        write!(self.stream, "\r\n")?;
        write!(self.stream, "{}", self.content)?;
        self.stream.flush()?;
        Ok(())
    }
}