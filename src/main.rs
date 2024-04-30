use std::collections::HashMap;
use std::io;
use std::io::{BufRead, Write};
// Uncomment this block to pass the first stage
use std::net::{TcpListener, TcpStream};
use itertools::Itertools;
use anyhow::Result;

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut _stream) => {
                println!("accepted new connection");
                let mut buf_reader = io::BufReader::new(&_stream);

                let mut buffer = String::new();
                buf_reader.read_line(&mut buffer).unwrap_or(0);

                let mut lines = buffer.lines();
                let first_line = lines.next().unwrap();
                // read headline
                let mut parts = first_line.split(' ');
                let _method = parts.next().filter(|&s| s == "GET" || s == "PUT");
                let path = parts.next().unwrap().split("/").collect::<Vec<&str>>();
                let _protocol = parts.next();
                println!("path: {:?}", path);
                // read headers
                let mut headers = HashMap::new();
                println!("path: {:?}", headers);
                loop {
                    let mut header = String::new();
                    if let Ok(_header_size) = buf_reader.read_line(&mut header) {
                        println!("header: {:?}", header);
                        let header = header.trim();
                        if header ==  "" {
                            break;
                        }
                        if let Some((key,value))=header.split(":").map(|s| s.trim()).collect_tuple() {
                            headers.insert(key.to_lowercase().to_string(),value.to_string());
                        } else {
                            continue;
                        }
                    }
                }
                let mut response = match path.as_slice() {
                    ["", ""] =>
                        Response::new(_stream),
                    ["", "user-agent"] =>
                        Response::new(_stream).with_header("Content-Type", "text/plain").with_content(headers.get("user-agent").unwrap_or(&"".to_string())),
                    ["", "echo", rest @ .. ] =>
                        {
                            let content = rest.join("/");
                            Response::new(_stream).with_header("Content-Type", "text/plain").with_content(&content)
                        },
                    _ =>
                        Response::new(_stream).with_code(404),
                };
                println!("response: {:?}",response);
                response.flush().unwrap()
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}

#[derive(Debug)]
struct Response {
    stream:  TcpStream,
    code: u16,
    content: String,
    pub headers: HashMap<String, String>,
}

impl Response {
    pub fn  new(stream: TcpStream) -> Self {
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
        for (k,v) in self.headers.iter() {
            write!(self.stream, "{}: {}\r\n", k,v)?;
        }
        write!(self.stream, "Content-Length: {}\r\n", self.content.len())?;
        write!(self.stream, "\r\n")?;
        write!(self.stream, "{}", self.content)?;
        self.stream.flush()?;
        Ok(())
    }
}