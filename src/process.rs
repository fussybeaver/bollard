use std::fmt::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct Process {
    pub user: String,
    pub pid: String,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub vsz: Option<String>,
    pub rss: Option<String>,
    pub tty: Option<String>,
    pub stat: Option<String>,
    pub start: Option<String>,
    pub time: Option<String>,
    pub command: String,
}

#[derive(Debug, RustcEncodable, RustcDecodable)]
#[allow(non_snake_case)]
pub struct Top {
    pub Titles: Vec<String>,
    pub Processes: Vec<Vec<String>>
}

impl Display for Process {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let mut s = String::new();

        s.push_str(&*self.user.clone());

        s.push_str(",");
        s.push_str(&*self.pid.clone());

        match self.cpu.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.memory.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.vsz.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.rss.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.tty.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.stat.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.start.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        match self.time.clone() {
            Some(v) => {
                s.push_str(",");
                s.push_str(&*v);
            },
            None => {}
        }

        s.push_str(",");
        s.push_str(&*self.command.clone());

        write!(f, "{}", s)
    }
}
