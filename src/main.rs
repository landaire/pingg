use std::io;

use termion::event::Key;
use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Axis, Block, Borders, Chart, Dataset, Marker, Widget};
use tui::Terminal;

use failure::{Error, Fail};
use event::*;

use std::process::Command;
use std::io::{BufReader, BufRead};

mod event;

struct App {
    received: Vec<(f64, f64)>,
    dropped: Vec<(f64, f64)>,
    max_latency: f64,
    max_seqnum: f64,
    ping_runner: PingRunner,
}

impl App {
    fn new() -> Result<App, failure::Error> {
        let runner = PingRunner::run(std::env::args().skip(1).collect())?;

        let received = vec![];
        let dropped = vec![];

        Ok(App {
            received,
            dropped,
            max_latency: 10.0,
            max_seqnum: 100.0,
            ping_runner: runner,
        })
    }

    fn update(&mut self) {
        for packet in self.ping_runner.by_ref().take(5) {
            match packet {
                Packet::Dropped{
                    sequence_num,
                    time,
                } => {
                    if sequence_num as f64 >= self.max_seqnum {
                        self.max_seqnum = sequence_num as f64 + 5.0;
                    }

                    if time >= self.max_latency {
                        self.max_latency = time + 5.0;
                    }

                    for i in 0..=sequence_num - self.dropped.len(){
                        self.dropped.push(((self.dropped.len() + i) as f64, -1.0));
                    }

                    self.dropped[sequence_num].1 = time;
                },
                Packet::Received {
                    sequence_num,
                    time,
                } => {
                    if sequence_num as f64 >= self.max_seqnum {
                        self.max_seqnum = sequence_num as f64 + 5.0;
                    }

                    if time >= self.max_latency {
                        self.max_latency = time + 5.0;
                    }

                    for i in 0..=sequence_num - self.received.len(){
                        self.received.push(((self.received.len() + i) as f64, -1.0));
                    }

                    self.received[sequence_num].1 = time;
                }
            }
        }
    }

    pub fn terminate(&mut self) {
        self.ping_runner.terminate();
    }
}

enum Packet {
    Received{sequence_num: usize, time: f64},
    Dropped{sequence_num: usize, time: f64},
}

struct PingRunner {
    child: std::process::Child,
    done: bool,
}

impl PingRunner {
    pub fn run(args: Vec<String>) -> Result<PingRunner, failure::Error> {
        let child = Command::new("ping")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        Ok(PingRunner {
            child,
            done: false,
        })
    }

    pub fn terminate(&mut self) {
        self.child.kill();
    }
}

impl Iterator for PingRunner {
    type Item = Packet;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let stdout = self.child.stdout.as_mut().unwrap();
        let mut stdout_reader = BufReader::new(stdout);

        let mut line = String::new();
        let result = stdout_reader.read_line(&mut line);
        if result.is_err() {
            return None;
        }

        if line.is_empty() || line.starts_with('-') {
            self.done = true;
            return None;
        }

        // parse the packet result
        let mut seq = 0usize;
        let mut time = 0.0f64;

        let parts: Vec<&str> = line.split_ascii_whitespace().collect();

        if parts.first().unwrap() == &"Request" {
            seq = parts.last().unwrap().parse::<usize>().unwrap();

            return Some(Packet::Dropped{
                sequence_num: seq,
                time,
            });
        }

        for part in parts {
            if part.starts_with("icmp_seq=") {
                seq = part["icmp_seq=".len()..].parse::<usize>().unwrap();
            } else if part.starts_with("time=") {
                time = part["time=".len()..].parse::<f64>().unwrap();
            }
        }

        Some(Packet::Received{
            sequence_num: seq,
            time,
        })
    }
}

fn main() -> Result<(), failure::Error> {
    // Terminal initialization
    let stdout = io::stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;


    // App
    let events = Events::new();
    let mut app = App::new()?;

    loop {
        terminal.draw(|mut f| {
            let size = f.size();
            Chart::default()
                .block(
                    Block::default()
                        .title("ICMP Packets")
                        .title_style(Style::default().fg(Color::Cyan).modifier(Modifier::BOLD))
                        .borders(Borders::ALL),
                )
                .x_axis(
                    Axis::default()
                        .title("Sequence Number")
                        .style(Style::default().fg(Color::Gray))
                        .labels_style(Style::default().modifier(Modifier::ITALIC))
                        .bounds([0.0, app.max_seqnum])
                        .labels(&[
                            &format!("{}", 0),
                            &format!("{}", app.max_seqnum / 2.0),
                            &format!("{}", app.max_seqnum),
                        ]),
                )
                .y_axis(
                    Axis::default()
                        .title("Latency (MS)")
                        .style(Style::default().fg(Color::Gray))
                        .labels_style(Style::default().modifier(Modifier::ITALIC))
                        .bounds([0.0, app.max_latency])
                        .labels(&["0", &format!("{}", app.max_latency/ 2.0), &format!("{}", app.max_latency)]),
                )
                .datasets(&[
                    Dataset::default()
                        .name("Received Packets")
                        .marker(Marker::Braille)
                        .style(Style::default().fg(Color::Cyan))
                        .data(&app.received),
                    Dataset::default()
                        .name("Dropped Packets")
                        .marker(Marker::Braille)
                        .style(Style::default().fg(Color::Red))
                        .data(&app.dropped),
                ])
                .render(&mut f, size);
        })?;

        match events.next()? {
            Event::Input(input) => {
                if input == Key::Char('q') {
                    app.terminate();
                    break;
                }
            }
            Event::Tick => {
                app.update();
            }
        }
    }

    Ok(())
}