use std::io;
use std::io::{BufRead, Read, Write};
// Uncomment this block to pass the first stage
use std::net::TcpListener;

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

                let mut parts = first_line.split(' ');
                let method = parts.next().filter(|&s|  s == "GET" || s == "PUT");
                let path = parts.next();
                let protocol = parts.next();
                println!("path: {:?}", path);
                match path {
                    Some("/") =>
                        write!(_stream, "HTTP/1.1 200 OK\r\n\r\n").unwrap(),
                    _ =>
                        write!(_stream, "HTTP/1.1 404 Not Found\r\n\r\n").unwrap(),
                }

                _stream.flush().unwrap()
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
