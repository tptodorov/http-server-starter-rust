use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

use anyhow::bail;
use flate2::bufread::GzEncoder;
use flate2::Compression;
use itertools::Itertools;

#[derive(Debug, Copy, Clone)]
pub enum Method {
    GET,
    POST,
}

impl TryFrom<&str> for Method {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            _ => bail!("invalid method {}", value)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Code {
    Ok = 200,
    Created = 201,
    NotFound = 404,
}

impl From<Code> for u16 {
    fn from(code: Code) -> Self {
        code as u16
    }
}

impl Code {
    fn as_str(&self) -> &str {
        match self {
            Code::Ok => "OK",
            Code::Created => "Created",
            Code::NotFound => "Not Found",
        }
    }
}

#[derive(Debug)]
pub struct Request {
    headers: HashMap<String, String>,
    pub method: Method,
    pub uri: String,
    pub path: Vec<String>,
    pub buf_reader: BufReader<TcpStream>,
}

impl Request {
    pub fn new(stream: TcpStream) -> anyhow::Result<Self> {
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
    pub fn write_content(&mut self, mut writer: BufWriter<File>, expected_len: Option<u32>) -> anyhow::Result<u32> {
        let content = &mut self.buf_reader;
        let mut buf = [0; 1024];
        let mut len = 0_u32;
        loop {
            if let Some(expected_len) = expected_len {
                if len >= expected_len {
                    break;
                }
            }
            match content.read(&mut buf) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    writer.write_all(&buf[..n])?;
                    len += n as u32;
                }
                Err(e) => bail!(e),
            }
        }
        Ok(len)
    }

    pub fn get_header(&self, header: &str) -> Option<&String> {
        let name = header.to_lowercase();
        self.headers.get(&name)
    }

    pub fn content_encodings(&self) -> Option<Vec<String>> {
        let encoding = self.headers.get("accept-encoding")
            .map(|v| v.to_lowercase());
        if let Some(encoding) = encoding {
            Some(encoding.split(",")
                .map(|e| e.trim().to_string())
                .collect())
        } else {
            None
        }
    }

    pub fn response(self) -> Response {
        let gzip = self.content_encodings().unwrap_or_default().contains(&"gzip".to_string());
        Response::new(self.buf_reader.into_inner(), gzip)
    }
}

#[derive(Debug)]
enum Content {
    Text(String),
    Bytes(BufReader<File>, u64),
}

#[derive(Debug)]
pub struct Response {
    stream: TcpStream,
    code: Code,
    content: Content,
    pub headers: HashMap<String, String>,
    gzip: bool,
}

impl Response {
    pub fn new(stream: TcpStream, gzip: bool) -> Self {
        let mut headers = HashMap::new();

        if gzip {
            headers.insert("Content-Encoding".to_string(), "gzip".to_string());
        }

        Response {
            stream,
            code: Code::Ok,
            content: Content::Text("".to_string()),
            headers,
            gzip,
        }
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    pub fn with_code(mut self, code: Code) -> Self {
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

    pub fn flush(&mut self) -> anyhow::Result<()> {
        let int_code: u16 = self.code.clone().into();
        write!(self.stream, "HTTP/1.1 {} {}\r\n", int_code, self.code.as_str())?;
        for (k, v) in self.headers.iter() {
            write!(self.stream, "{}: {}\r\n", k, v)?;
        }
        match self.content {
            Content::Text(ref content) => {
                if self.gzip {
                    let mut encoder = GzEncoder::new(content.as_bytes(), Compression::default());
                    let mut gzip_content = Vec::new();
                    encoder.read_to_end(&mut gzip_content)?;
                    write!(self.stream, "Content-Length: {}\r\n", gzip_content.len())?;
                    write!(self.stream, "\r\n")?;
                    self.stream.write_all(&gzip_content)?;
                } else {
                    // plain encoding
                    write!(self.stream, "Content-Length: {}\r\n", content.len())?;
                    write!(self.stream, "\r\n")?;
                    write!(self.stream, "{}", content)?;
                }
            }
            Content::Bytes(ref mut content, size) => {
                if self.gzip {
                    let mut encoder = GzEncoder::new(content, Compression::default());
                    let mut gzip_content = Vec::new();
                    encoder.read_to_end(&mut gzip_content)?;
                    write!(self.stream, "Content-Length: {}\r\n", gzip_content.len())?;
                    write!(self.stream, "\r\n")?;
                    self.stream.write_all(&gzip_content)?;
                } else {
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
        }

        self.stream.flush()?;
        Ok(())
    }
}
