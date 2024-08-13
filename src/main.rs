//! Parse input from stdin and log actions on stdout
use crossterm::{
    cursor::{Hide, MoveTo, MoveToNextLine, Show},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};

use clap::Parser as ClapParser;

use std::io::{self, Read, Write};

use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};

#[derive(ClapParser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Treat border characters as static
    #[arg(short, long)]
    borders: bool,

    /// Treat these colors as static (can specify multiple)
    #[arg(short, long)]
    color: Vec<u32>,
    /// List all of the colors piped in, and do nothing else
    #[arg(long)]
    list_colors: bool,

    /// Enable anti-gravity
    #[arg(short, long)]
    antigravity: bool,
}

/// This thing parses the initial input using anstyle-parse
struct Performer {
    grid: Grid,
    x: usize,
    y: usize,
    fg: u32,
    colors: std::collections::HashSet<u32>,
}

impl Perform for Performer {
    fn print(&mut self, c: char) {
        let cell = self.grid.get_mut(self.x, self.y);
        cell.c = c;
        cell.fg = self.fg;
        self.x += 1;
    }

    fn execute(&mut self, byte: u8) {
        if byte == 0x0a {
            self.y += 1;
            self.x = 0;
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: u8) {
        // println!(
        //     "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
        //     params, intermediates, ignore, c
        // );
        let items: Vec<_> = params.iter().collect();
        {
            match items[0][0] {
                0 => {
                    self.fg = 15;
                    self.colors.insert(self.fg);
                }
                30..=37 => {
                    self.fg = (items[0][0] - 30) as u32;
                    self.colors.insert(self.fg);
                }
                38 => {
                    if items[1][0] == 5 {
                        self.fg = items[2][0] as u32;
                        self.colors.insert(self.fg);
                    } else if items[1][0] == 2 {
                        self.fg = (1 << 31)
                            ^ ((items[2][0] as u32) << 16)
                            ^ ((items[3][0] as u32) << 8)
                            ^ (items[4][0] as u32);
                        self.colors.insert(self.fg);
                    }
                }
                39 => {
                    self.fg = 15;
                    self.colors.insert(self.fg);
                }
                90..=97 => {
                    self.fg = (items[0][0] - 82) as u32;
                    self.colors.insert(self.fg);
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone)]
struct Cell {
    fg: u32,
    c: char,
}
struct Grid {
    width: usize,
    height: usize,
    data: Box<[Cell]>,
    args: Args,
}

fn is_box_char(data: &char) -> bool {
    match *data {
        '\u{2500}'..='\u{257F}' => true,
        _ => false,
    }
}

fn write_color(lock: &mut io::StdoutLock<'static>, fg: u32) {
    if fg < (1 << 31) {
        write!(lock, "\x1b[38;5;{}m", fg).unwrap();
    } else {
        let r = ((fg >> 16) & 0xFF) as u8;
        let g = ((fg >> 8) & 0xFF) as u8;
        let b = ((fg) & 0xFF) as u8;
        write!(lock, "\x1b[38;2;{};{};{}m", r, g, b).unwrap();
    }
}

impl Grid {
    fn new(args: Args, w: usize, h: usize) -> Self {
        Grid {
            args,
            width: w,
            height: h,
            data: vec![Cell { fg: 0, c: ' ' }; w * h].into_boxed_slice(),
        }
    }

    fn get_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        &mut self.data[y * self.width + x]
    }

    fn swap(&mut self, x1: usize, y1: usize, x2: usize, y2: usize) {
        let idx1 = y1 * self.width + x1;
        let idx2 = y2 * self.width + x2;

        self.data.swap(idx1, idx2);
        let cell_a = self.get_mut(x1, y1);
        if cell_a.c == '\0' {
            cell_a.c = ' ';
        }
        let cell_b = self.get_mut(x2, y2);
        if cell_b.c == '\0' {
            cell_b.c = ' ';
        }
    }
    fn is_static(&self, x: usize, y: usize) -> bool {
        if self.args.borders {
            if is_box_char(&self.data[y * self.width + x].c) {
                return true;
            }
        }
        !self.is_empty(x, y) && self.args.color.contains(&self.data[y * self.width + x].fg)

        // some colors from tokyonight-storm:
        //
        // Line numbers: 2151367265
        // Number literals:  2164235876
        // Function names: 2150286302
        // Punctuation:  2156518911
        // namespaces: 2155728895
        // braces: 2158604758
        // keywords:  2157804760
        // white text:  2160118517
        // members:  2155076298
        // Inactive filenames:  2155051682
        // comments: 2153144201
    }

    fn is_sand(&self, x: usize, y: usize) -> bool {
        !self.is_empty(x, y) && !self.is_static(x, y)
    }

    fn is_empty(&self, x: usize, y: usize) -> bool {
        let cell = &self.data[y * self.width + x];
        if cell.c == '\0' || cell.c == ' ' {
            return true;
        }
        return false;
    }

    fn render(&self) {
        let mut fg = 0;
        let mut lock = io::stdout().lock();
        for y in 0..self.height {
            for x in 0..self.width {
                let d = &self.data[y * self.width + x];
                if fg != d.fg {
                    fg = d.fg;
                    write_color(&mut lock, fg);
                }
                if d.c == '\0' && y < self.height - 1 {
                    execute!(lock, MoveToNextLine(1)).unwrap();
                    break;
                }
                write!(lock, "{}", d.c).unwrap();
            }
        }
        execute!(lock, MoveTo(0, 0)).unwrap();
    }
    fn step(&mut self) {
        let range: Box<dyn Iterator<Item = usize>> = match self.args.antigravity {
            true => Box::new(0..self.height - 1),
            false => Box::new((1..self.height).rev()),
        };
        for y in range {
            let delta: usize = match self.args.antigravity {
                true => y + 1,
                false => y - 1,
            };
            for x in 0..self.width {
                if self.is_sand(x, delta) {
                    let rand_choice = rand::random::<f32>();

                    if self.is_empty(x, y) && !self.is_static(x, y) {
                        self.swap(x, y, x, delta);
                    } else if rand_choice < 0.5
                        && x > 0
                        && self.is_empty(x - 1, y)
                        && !self.is_static(x - 1, y)
                    {
                        self.swap(x - 1, y, x, delta);
                    } else if rand_choice >= 0.5
                        && x < self.width - 1
                        && self.is_empty(x + 1, y)
                        && !self.is_static(x + 1, y)
                    {
                        self.swap(x + 1, y, x, delta);
                    }
                }
            }
        }
    }
}

fn main() {
    let args = Args::parse();

    let Some((w, h)) = term_size::dimensions() else {
        panic!("unable to get term dimensions");
    };
    let input = io::stdin();
    let mut handle = input.lock();

    let mut statemachine = Parser::<DefaultCharAccumulator>::new();
    let mut performer = Performer {
        grid: Grid::new(args, w, h),
        x: 0,
        y: 0,
        fg: 15,
        colors: std::collections::HashSet::new(),
    };

    let mut buf = [0; 2048];

    loop {
        match handle.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for byte in &buf[..n] {
                    statemachine.advance(&mut performer, *byte);
                }
            }
            Err(_err) => {
                break;
            }
        }
    }

    if performer.grid.args.list_colors {
        let mut lock = io::stdout().lock();
        write!(lock, "Colors detected in input:\n").unwrap();
        for color in performer.colors.iter() {
            write_color(&mut lock, *color);
            write!(lock, "  ***** {}\n", color).unwrap();
        }
        write!(lock, "\x1b[39m\n").unwrap();
        return;
    }

    execute!(io::stdout(), EnterAlternateScreen, Hide, MoveTo(0, 0)).unwrap();
    enable_raw_mode().unwrap();
    performer.grid.render();
    std::thread::sleep(std::time::Duration::from_millis(400));
    for _ in 0..100 {
        {
            let grid = &mut performer.grid;
            grid.step();
        }
        {
            execute!(io::stdout(), MoveTo(0, 0), BeginSynchronizedUpdate).unwrap();
            let grid = &performer.grid;
            grid.render();
            execute!(io::stdout(), EndSynchronizedUpdate).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    disable_raw_mode().unwrap();
    execute!(io::stdout(), LeaveAlternateScreen, Show).unwrap();
}
