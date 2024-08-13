//! Parse input from stdin and log actions on stdout
use crossterm::{
    cursor::{Hide, MoveTo, MoveToNextLine, Show},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use std::io::{self, Read, Write};

use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};

/// This thing parses the initial input using anstyle-parse
struct Performer {
    grid: Grid,
    x: usize,
    y: usize,
    fg: u32,
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
                }
                30..=37 => {
                    self.fg = (items[0][0] - 30) as u32;
                }
                38 => {
                    if items[1][0] == 5 {
                        self.fg = items[2][0] as u32;
                    } else if items[1][0] == 2 {
                        self.fg = (1 << 31)
                            ^ ((items[2][0] as u32) << 16)
                            ^ ((items[3][0] as u32) << 8)
                            ^ (items[4][0] as u32);
                    }
                }
                39 => {
                    self.fg = 15;
                }
                90..=97 => {
                    self.fg = (items[0][0] - 82) as u32;
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
}

impl Grid {
    fn new(w: usize, h: usize) -> Self {
        Grid {
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
        // Hard-coded to the color of comments in my terminal kekw
        !self.is_empty(x, y) && self.data[y * self.width + x].fg == 2153144201

        // some other colors from my theme:
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
        // Inactive filenames  2155051682
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
                    if fg < (1 << 31) {
                        write!(lock, "\x1b[38;5;{}m", fg).unwrap();
                    } else {
                        let r = ((fg >> 16) & 0xFF) as u8;
                        let g = ((fg >> 8) & 0xFF) as u8;
                        let b = ((fg) & 0xFF) as u8;
                        write!(lock, "\x1b[38;2;{};{};{}m", r, g, b).unwrap();
                    }
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
        for y in (1..self.height).rev() {
            for x in 0..self.width {
                if self.is_sand(x, y - 1) {
                    let rand_choice = rand::random::<f32>();

                    if self.is_empty(x, y) && !self.is_static(x, y) {
                        self.swap(x, y, x, y - 1);
                    } else if rand_choice < 0.5
                        && x > 0
                        && self.is_empty(x - 1, y)
                        && !self.is_static(x - 1, y)
                    {
                        self.swap(x - 1, y, x, y - 1);
                    } else if rand_choice >= 0.5
                        && x < self.width - 1
                        && self.is_empty(x + 1, y)
                        && !self.is_static(x + 1, y)
                    {
                        self.swap(x + 1, y, x, y - 1);
                    }
                }
            }
        }
    }
}

fn main() {
    let Some((w, h)) = term_size::dimensions() else {
        panic!("unable to get term dimensions");
    };
    let input = io::stdin();
    let mut handle = input.lock();

    let mut statemachine = Parser::<DefaultCharAccumulator>::new();
    let mut performer = Performer {
        grid: Grid::new(w, h),
        x: 0,
        y: 0,
        fg: 15,
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
