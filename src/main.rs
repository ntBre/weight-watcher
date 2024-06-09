use std::{
    fmt::Display,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
};

enum ContentType {
    Html,
}

impl Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Html => write!(f, "text/html"),
        }
    }
}

struct Response {
    status: usize,
    content_type: ContentType,
    body: String,
}

impl Response {
    fn ok() -> Self {
        Self {
            status: 200,
            body: String::new(),
            content_type: ContentType::Html,
        }
    }

    fn body(mut self, body: String) -> Self {
        self.body = body;
        self
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            _ => "",
        }
    }

    fn as_bytes(&self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP/1.1 {} {}\r\n", self.status, self.reason())?;
        write!(f, "Content-Type: {}\r\n", self.content_type)?;
        write!(f, "\r\n")?;
        write!(f, "{}", self.body)?;
        Ok(())
    }
}

fn handle_client(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&mut stream);
    let request: Vec<_> = buf_reader
        .lines()
        .map(Result::unwrap)
        .take_while(|line| !line.is_empty())
        .collect();
    println!("{}", request[0]);
    let response = Response::ok().body("<h1>Hello, world!</h1>".into());
    stream.write_all(&response.as_bytes()).unwrap();
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9999")?;

    for stream in listener.incoming().map(Result::unwrap) {
        handle_client(stream);
    }
    Ok(())
}
