use std::{
    fmt::Display,
    fs::File,
    io::{BufRead, BufReader, Read, Seek, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
        "/favicon.ico" => Response::ok()
            .content_type(ContentType::Png)
            .body(Body::Bytes(include_bytes!("../logo.png").to_vec())),
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
    state.update();
    let tmpl = include_str!("../templates/index.html")
        .replace("{{table}}", &state.html_table());
    state.graph();
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
    let date = format_date(&now);
    writeln!(state.outfile, "{date} {weight:.1}",).unwrap();
    state.data.push((date, weight));
    Response::redirect("/")
}

fn format_date(date: &OffsetDateTime) -> String {
    format!(
        "{}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

struct State {
    data: Vec<(String, f64)>,
    config_file: PathBuf,
    outfile: File,
}

impl State {
    fn update(&mut self) {
        self.data = load_current(&mut self.outfile);
    }

    fn html_table(&self) -> String {
        use std::fmt::Write;
        let mut table = String::new();
        for (date, weight) in self.data.iter().rev().take(7) {
            writeln!(table, "<tr><td>{date}</td><td>{weight:.1}</td></tr>")
                .unwrap();
        }
        table
    }

    fn minmax(&self) -> (Option<f64>, Option<f64>) {
        let mut weights: Vec<_> = self.data.iter().map(|p| p.1).collect();
        weights.sort_by(f64::total_cmp);
        let min = weights.first().cloned();
        let max = weights.last().cloned();
        (min, max)
    }

    fn graph(&self) {
        let name = self.config_file.to_str().unwrap();
        let now = OffsetDateTime::now_local().unwrap();
        let start_date = now - 28 * time::Duration::DAY;
        let date_start = format_date(&start_date);
        let date_end = format_date(&(now + time::Duration::DAY));

        let mut gp_script = include_str!("plot.gp")
            .replace("{{name}}", name)
            .replace("{{date_start}}", &date_start)
            .replace("{{date_end}}", &date_end);
        const WEIGHT_PAD: f64 = 5.0;
        if let (Some(weight_start), Some(weight_end)) = self.minmax() {
            let weight_start = weight_start - WEIGHT_PAD;
            let weight_end = weight_end + WEIGHT_PAD;
            let weight_range =
                format!("set yrange [{}:{}]", weight_start, weight_end);
            gp_script = gp_script.replace("{{yrange}}", &weight_range);
        } else {
            gp_script = gp_script.replace("{{yrange}}", "");
        }

        let mut child = Command::new("gnuplot")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            stdin.write_all(gp_script.as_bytes()).unwrap();
        });
        let output = child.wait().unwrap();
        if output.code() != Some(0) {
            eprintln!("error running gnuplot");
        }
    }
}

fn load_current(config: &mut File) -> Vec<(String, f64)> {
    config.rewind().unwrap();
    let mut contents = String::new();
    config.read_to_string(&mut contents).unwrap();
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
        .open(&config_file)
        .expect("failed to open weights file");

    let cur = load_current(&mut config);

    let mut state = State {
        data: cur,
        outfile: config,
        config_file,
    };

    let listener = TcpListener::bind("0.0.0.0:9999")?;

    for stream in listener.incoming().map(Result::unwrap) {
        dispatch(stream, &mut state);
    }
    Ok(())
}
