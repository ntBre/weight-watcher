use std::{
    fmt::Display,
    fs::{read_to_string, File},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
};

use time::OffsetDateTime;

enum ContentType {
    Html,
    Png,
}

impl Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Html => write!(f, "text/html"),
            ContentType::Png => write!(f, "image/png"),
        }
    }
}

enum Body {
    String(String),
    Bytes(Vec<u8>),
}

impl From<&str> for Body {
    fn from(value: &str) -> Self {
        Self::String(value.into())
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

struct Response {
    status: usize,
    location: Option<&'static str>,
    content_type: ContentType,
    body: Body,
}

impl Response {
    fn ok() -> Self {
        Self {
            status: 200,
            body: Body::String(String::new()),
            content_type: ContentType::Html,
            location: None,
        }
    }

    fn redirect(to: &'static str) -> Self {
        Self {
            status: 303,
            location: Some(to),
            body: Body::String(String::new()),
            content_type: ContentType::Html,
        }
    }

    fn err() -> Self {
        Self {
            status: 404,
            body: Body::String(String::new()),
            content_type: ContentType::Html,
            location: None,
        }
    }

    fn body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }

    fn content_type(mut self, content_type: ContentType) -> Self {
        self.content_type = content_type;
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
        let mut header = self.to_string().into_bytes();

        match &self.body {
            Body::String(s) => header.extend(s.as_bytes()),
            Body::Bytes(bytes) => header.extend(bytes),
        }

        header
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

        Ok(())
    }
}

fn dispatch(mut stream: TcpStream, state: &mut State) {
    let buf_reader = BufReader::new(&mut stream);
    let request: Vec<_> = buf_reader
        .lines()
        .map(Result::unwrap)
        .take_while(|line| !line.is_empty())
        .collect();
    assert!(!request.is_empty());
    let fields: Vec<_> = request[0].split_ascii_whitespace().collect();
    assert!(fields.len() == 3);
    let url = fields[1];
    let parts: Vec<_> = url.split('?').collect();
    assert!(matches!(parts.len(), 1 | 2));
    let response = match parts[0] {
        "/" => index(state),
        "/weight" if parts.len() == 2 => weight(parts[1], state),
        f @ "/tmp/weight-watcher.png" => Response::ok()
            .content_type(ContentType::Png)
            .body(Body::Bytes(std::fs::read(f).unwrap())),
        _ => {
            Response::err().body(include_str!("../templates/error.html").into())
        }
    };
    stream.write_all(&response.as_bytes()).unwrap();
}

fn index(state: &mut State) -> Response {
    use std::fmt::Write;
    let mut table = String::new();
    for (date, weight) in state.data.iter().rev().take(7) {
        writeln!(table, "<tr><td>{date}</td><td>{weight:.1}</td></tr>")
            .unwrap();
    }
    let tmpl = read_to_string("templates/index.html")
        .unwrap()
        .replace("{{table}}", &table);
    Response::ok().body(tmpl.into())
}

fn weight(query: &str, state: &mut State) -> Response {
    let params: Vec<&str> = query.split('=').collect();
    if params.len() != 2 {
        return Response::err();
    }
    let Ok(weight) = params[1].parse::<f64>() else {
        return Response::err();
    };
    let now = OffsetDateTime::now_local().unwrap();
    let date =
        format!("{}-{:02}-{:02}", now.year(), now.month() as u8, now.day());
    writeln!(state.outfile, "{date} {weight:.1}",).unwrap();
    state.data.push((date, weight));
    Response::redirect("/")
}

struct State {
    data: Vec<(String, f64)>,
    outfile: File,
}

fn load_current(contents: String) -> Vec<(String, f64)> {
    contents
        .lines()
        .flat_map(|line| {
            let sp: Vec<_> = line.split_ascii_whitespace().collect();
            if sp.len() != 2 {
                return None;
            }
            let date = sp[0].to_owned();
            let Ok(weight) = sp[1].parse::<f64>() else {
                return None;
            };
            Some((date, weight))
        })
        .collect()
}

fn main() -> std::io::Result<()> {
    let home = std::env::var("HOME").unwrap();
    let home = Path::new(&home);
    let config_dir = home.join(".config").join("weight-watcher");
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)
            .expect("failed to create config dir");
    }

    let config_file = config_dir.join("weights.dat");
    let mut config = File::options()
        .create(true)
        .read(true)
        .append(true)
        .open(config_file)
        .expect("failed to open weights file");

    let mut contents = String::new();
    config.read_to_string(&mut contents).unwrap();

    let cur = load_current(contents);

    let mut state = State {
        data: cur,
        outfile: config,
    };

    let listener = TcpListener::bind("0.0.0.0:9999")?;

    for stream in listener.incoming().map(Result::unwrap) {
        dispatch(stream, &mut state);
    }
    Ok(())
}
