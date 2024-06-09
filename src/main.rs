use std::{
    fmt::Display,
    fs::File,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::Path,
};

use time::OffsetDateTime;

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
    location: Option<&'static str>,
    content_type: ContentType,
    body: String,
}

impl Response {
    fn ok() -> Self {
        Self {
            status: 200,
            body: String::new(),
            content_type: ContentType::Html,
            location: None,
        }
    }

    fn redirect(to: &'static str) -> Self {
        Self {
            status: 303,
            location: Some(to),
            body: String::new(),
            content_type: ContentType::Html,
        }
    }

    fn err() -> Self {
        Self {
            status: 404,
            body: String::new(),
            content_type: ContentType::Html,
            location: None,
        }
    }

    fn body(mut self, body: String) -> Self {
        self.body = body;
        self
    }

    fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            303 => "See Other",
            404 => "Not Found",
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
        if let Some(location) = self.location {
            write!(f, "Location: {}", location)?;
        } else {
            write!(f, "Content-Type: {}\r\n", self.content_type)?;
        }
        write!(f, "\r\n")?;
        write!(f, "{}", self.body)?;
        Ok(())
    }
}

fn dispatch(mut stream: TcpStream, outfile: &mut File) {
    let buf_reader = BufReader::new(&mut stream);
    let request: Vec<_> = buf_reader
        .lines()
        .map(Result::unwrap)
        .take_while(|line| !line.is_empty())
        .collect();
    assert!(request.len() >= 1);
    let fields: Vec<_> = request[0].split_ascii_whitespace().collect();
    assert!(fields.len() == 3);
    let url = fields[1];
    let parts: Vec<_> = url.split('?').collect();
    assert!(matches!(parts.len(), 1 | 2));
    let response = match parts[0] {
        "/" => index(),
        "/weight" if parts.len() == 2 => weight(parts[1], outfile),
        _ => {
            Response::err().body(include_str!("../templates/error.html").into())
        }
    };
    stream.write_all(&response.as_bytes()).unwrap();
}

fn index() -> Response {
    Response::ok().body(include_str!("../templates/index.html").into())
}

fn weight(query: &str, outfile: &mut File) -> Response {
    let params: Vec<&str> = query.split('=').collect();
    if params.len() != 2 {
        return Response::err();
    }
    let Ok(w) = params[1].parse::<f64>() else {
        return Response::err();
    };
    let now = OffsetDateTime::now_local().unwrap();
    writeln!(
        outfile,
        "{}-{:02}-{:02} {w:.1}",
        now.year(),
        now.month() as u8,
        now.day()
    )
    .unwrap();
    Response::redirect("/")
}

fn main() -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap();
    let home = Path::new(&home);
    let config_dir = home.join(".config").join("weight-watcher");
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)
            .expect("failed to create config dir");
    }

    let config = config_dir.join("weights.dat");
    let mut config = File::options()
        .create(true)
        .append(true)
        .open(config)
        .expect("failed to open weights file");

    let listener = TcpListener::bind("0.0.0.0:9999")?;

    for stream in listener.incoming().map(Result::unwrap) {
        dispatch(stream, &mut config);
    }
    Ok(())
}
