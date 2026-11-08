use crate::terminal::{self, Colors};

pub enum Msg {
    Done(&'static str, String, String),
    Planned(&'static str, String, String),
    Skipping(String, String),
    Note(String),
    Error(String),
}

impl Msg {
    fn render(&self, clr: &Colors) -> String {
        let (color, verb, target, reason) = match self {
            Msg::Done(verb, target, reason) => (clr.green, *verb, target, reason),
            Msg::Planned(verb, target, reason) => (clr.yellow, *verb, target, reason),
            Msg::Skipping(target, reason) => (clr.yellow, "skipping", target, reason),
            Msg::Note(text) => return format!("{}{text}{}", clr.dim, clr.reset),
            Msg::Error(text) => return format!("{}{text}{}", clr.red, clr.reset),
        };
        format!(
            "{color}{verb}{} {target} {}({reason}){}",
            clr.reset, clr.dim, clr.reset
        )
    }
}

pub struct Printer {
    clr: Colors,
    started: bool,
}

impl Printer {
    pub fn new() -> Self {
        Self {
            clr: terminal::stderr_colors(),
            started: false,
        }
    }

    pub fn group(&mut self, header: Option<String>) -> Group<'_> {
        Group {
            printer: self,
            header,
            opened: false,
        }
    }
}

pub struct Group<'a> {
    printer: &'a mut Printer,
    header: Option<String>,
    opened: bool,
}

impl Group<'_> {
    pub fn emit(&mut self, msg: Msg) {
        let clr = &self.printer.clr;
        if !self.opened {
            if self.printer.started {
                eprintln!();
            }
            if let Some(header) = &self.header {
                eprintln!("{}{header}:{}", clr.bold, clr.reset);
            }
            self.printer.started = true;
            self.opened = true;
        }
        let indent = if self.header.is_some() { "  " } else { "" };
        eprintln!("{indent}{}", msg.render(clr));
    }
}
