//! Parse input from stdin and log actions on stdout
use crossterm::{
    cursor::{Hide, MoveTo, MoveToNextLine, Show},
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};

use rand::seq::SliceRandom;
use rand::Rng;

use clap::Parser as ClapParser;
use unicode_width::UnicodeWidthChar;

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

    /// Treat these background colors as static (can specify multiple)
    #[arg(long)]
    bg: Vec<u32>,

    /// Characters used for snow
    #[arg(short, long)]
    snow_chars: Vec<char>,

    /// List all of the colors piped in, and do nothing else
    #[arg(long)]
    list_colors: bool,

    /// Enable anti-gravity
    #[arg(short, long)]
    antigravity: bool,

    /// Enable snow effect
    #[arg(long)]
    snow: bool,

    /// Enable blender effect
    #[arg(long, default_value_t = 0)]
    blender: usize,

    /// How many milliseconds to sleep for
    #[arg(long, default_value_t = 30)]
    ms: u64,
}

struct Vec2 {
    x: isize,
    y: isize,
}

/// This thing parses the initial input using anstyle-parse
struct Performer {
    grid: Grid,
    x: usize,
    y: usize,
    fg: u32,
    bg: u32,
    colors: std::collections::HashSet<u32>,
    bg_colors: std::collections::HashSet<u32>,
}

impl Perform for Performer {
    fn print(&mut self, c: char) {
        if self.x >= self.grid.width {
            return;
        }
        if self.y >= self.grid.height {
            return;
        }
        let cell = self.grid.get_mut(self.x, self.y);
        cell.c = c;
        cell.fg = self.fg;
        cell.bg = self.bg;
        let mut width = UnicodeWidthChar::width(c).unwrap();
        while width > 0 {
            self.x += 1;
            self.grid.get_mut(self.x, self.y).bg = self.bg;
            width -= 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        if byte == 0x0a {
            self.grid.splat(self.x, self.y, self.bg);
            self.y += 1;
            // self.bg = u32::MAX;
            self.x = 0;
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: u8) {
        // println!(
        //     "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
        //     params, intermediates, ignore, c
        // );
        //
        // if c != b'm' {
        //     return;
        // }
        let items: Vec<_> = params.iter().collect();
        {
            match items[0][0] {
                0 => {
                    self.fg = 15;
                    self.bg = u32::MAX;
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
                40..=47 => {
                    self.bg = (items[0][0] - 40) as u32;
                    self.bg_colors.insert(self.bg);
                }
                48 => {
                    if items[1][0] == 5 {
                        self.bg = items[2][0] as u32;
                    } else if items[1][0] == 2 {
                        self.bg = (1 << 31)
                            ^ ((items[2][0] as u32) << 16)
                            ^ ((items[3][0] as u32) << 8)
                            ^ (items[4][0] as u32);
                    }
                    self.bg_colors.insert(self.bg);
                }
                49 => {
                    self.bg = u32::MAX;
                }
                90..=97 => {
                    self.fg = (items[0][0] - 82) as u32;
                    self.colors.insert(self.fg);
                }
                100..=107 => {
                    self.bg = (items[0][0] - 92) as u32;
                    self.bg_colors.insert(self.bg);
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone)]
struct Cell {
    fg: u32,
    bg: u32,
    c: char,
}
struct Grid {
    width: usize,
    height: usize,
    data: Box<[Cell]>,
    args: Args,
    snow_fg: SnowGrid,
    snow_bg: SnowGrid,
}
#[derive(Clone)]
struct Snowflake {
    c: char,
}

struct SnowGrid {
    width: usize,
    height: usize,
    data: Box<[Snowflake]>,
    flip: usize,
    flip_rate: usize,
}

fn is_box_char(data: &char) -> bool {
    match *data {
        '\u{2500}'..='\u{257F}' => true,
        _ => false,
    }
}

fn write_bg_color(lock: &mut io::StdoutLock<'static>, bg: u32) {
    if bg == u32::MAX {
        write!(lock, "\x1b[49;m").unwrap();
    } else if bg < (1 << 31) {
        write!(lock, "\x1b[48;5;{}m", bg).unwrap();
    } else {
        let r = ((bg >> 16) & 0xFF) as u8;
        let g = ((bg >> 8) & 0xFF) as u8;
        let b = ((bg) & 0xFF) as u8;
        write!(lock, "\x1b[48;2;{};{};{}m", r, g, b).unwrap();
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

impl SnowGrid {
    fn get(&self, x: usize, y: usize) -> Option<char> {
        match self.is_empty(x, y) {
            true => None,
            false => Some(self.data[y * self.width + x].c),
        }
    }
    fn get_mut(&mut self, x: usize, y: usize) -> &mut Snowflake {
        &mut self.data[y * self.width + x]
    }
    fn step(&mut self, args: &Args) {
        self.flip = (self.flip + 1) % self.flip_rate;
        if self.flip != 0 {
            return;
        }
        // spawn
        let mut rng = rand::thread_rng();
        let rand_x = rng.gen_range(0..self.width);
        self.get_mut(
            rand_x,
            match args.antigravity {
                true => self.height - 1,
                false => 0,
            },
        )
        .c = *args.snow_chars.choose(&mut rng).unwrap();
        let range: Box<dyn Iterator<Item = usize>> = match args.antigravity {
            true => Box::new(0..self.height - 1),
            false => Box::new((1..self.height).rev()),
        };
        for y in range {
            let delta: usize = match args.antigravity {
                true => y + 1,
                false => y - 1,
            };
            for x in 0..self.width {
                if !self.is_empty(x, delta) {
                    let rand_choice = rand::random::<f32>();

                    if rand_choice < 0.2 && x > 0 && self.is_empty(x - 1, y) {
                        self.swap(x - 1, y, x, delta);
                    } else if rand_choice >= 0.8 && x < self.width - 1 && self.is_empty(x + 1, y) {
                        self.swap(x + 1, y, x, delta);
                    } else if self.is_empty(x, y) {
                        self.swap(x, y, x, delta);
                    }
                }
            }
        }
    }
    fn is_empty(&self, x: usize, y: usize) -> bool {
        let cell = &self.data[y * self.width + x];
        if cell.c == ' ' {
            return true;
        }
        return false;
    }
    fn swap(&mut self, x1: usize, y1: usize, x2: usize, y2: usize) {
        let idx1 = y1 * self.width + x1;
        let idx2 = y2 * self.width + x2;

        self.data.swap(idx1, idx2);
    }
}
impl Grid {
    fn new(args: Args, w: usize, h: usize) -> Self {
        Grid {
            args,
            width: w,
            height: h,
            data: vec![
                Cell {
                    fg: 0,
                    bg: u32::MAX,
                    c: ' '
                };
                w * h
            ]
            .into_boxed_slice(),
            snow_fg: SnowGrid {
                width: w,
                height: h,
                data: vec![Snowflake { c: ' ' }; w * h].into_boxed_slice(),
                flip: 0,
                flip_rate: 1,
            },
            snow_bg: SnowGrid {
                width: w,
                height: h,
                data: vec![Snowflake { c: ' ' }; w * h].into_boxed_slice(),
                flip: 0,
                flip_rate: 3,
            },
        }
    }

    fn splat(&mut self, x: usize, y: usize, bg: u32) {
        // smear the background to the end of the row
        if x == 0 && y == 0 || bg == u32::MAX {
            return;
        }
        let mut i = x;
        while i < self.width {
            self.get_mut(i, y).bg = bg;
            i += 1;
        }
    }
    fn get_mut(&mut self, x: usize, y: usize) -> &mut Cell {
        &mut self.data[y * self.width + x]
    }

    fn swap(&mut self, x1: usize, y1: usize, x2: usize, y2: usize) {
        let idx1 = y1 * self.width + x1;
        let idx2 = y2 * self.width + x2;

        self.data.swap(idx1, idx2);
        let (a, b) = if idx1 < idx2 {
            let (left, right) = self.data.split_at_mut(idx2);
            (&mut left[idx1], &mut right[0])
        } else {
            let (left, right) = self.data.split_at_mut(idx1);
            (&mut right[0], &mut left[idx2])
        };

        if a.c == '\0' {
            a.c = ' ';
        }
        if b.c == '\0' {
            b.c = ' ';
        }

        // swap backgrounds
        std::mem::swap(&mut a.bg, &mut b.bg);
    }
    fn is_static(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        if self.args.borders {
            if is_box_char(&self.data[y * self.width + x].c) {
                return true;
            }
        }
        let fg = self.data[y * self.width + x].fg;
        let bg = self.data[y * self.width + x].bg;
        !self.is_empty(x, y) && (self.args.color.contains(&fg) || self.args.bg.contains(&bg))

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
        y < self.height && x < self.width && !self.is_empty(x, y) && !self.is_static(x, y)
    }

    fn is_empty(&self, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let cell = &self.data[y * self.width + x];
        if cell.c == '\0' || cell.c == ' ' {
            return true;
        }
        return false;
    }

    fn render(&self) {
        let mut fg = 0;
        let mut bg = 0;
        let mut lock = io::stdout().lock();
        for y in 0..self.height {
            let mut skip = 0;
            for x in 0..self.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let d = &self.data[y * self.width + x];
                let snow_fg = self.snow_fg.get(x, y);
                let snow_bg = self.snow_bg.get(x, y);
                if snow_fg.is_some() || (snow_bg.is_some() && d.c == ' ') {
                    if fg != 15 {
                        fg = 15;
                        write_color(&mut lock, fg);
                    }
                } else if fg != d.fg {
                    fg = d.fg;
                    write_color(&mut lock, fg);
                }
                if bg != d.bg {
                    bg = d.bg;
                    write_bg_color(&mut lock, bg);
                }
                if d.c == '\0' && y < self.height - 1 {
                    execute!(lock, MoveToNextLine(1)).unwrap();
                    break;
                }
                if let Some(snow_fg) = snow_fg {
                    write!(lock, "{}", snow_fg).unwrap();
                } else {
                    if d.c == ' ' && snow_bg.is_some() {
                        write!(lock, "{}", snow_bg.unwrap()).unwrap();
                    } else {
                        write!(lock, "{}", d.c).unwrap();
                        let width = UnicodeWidthChar::width(d.c);
                        if let Some(width) = width {
                            if width > 1 {
                                skip = width - 1;
                            }
                        }
                    }
                }
            }
        }
        execute!(lock, MoveTo(0, 0)).unwrap();
    }

    fn snow_step(&mut self) {
        self.snow_fg.step(&self.args);
        self.snow_bg.step(&self.args);
    }

    fn step(&mut self, dir: &Vec2) {
        let yrange: Box<dyn Iterator<Item = usize>> = match dir.y {
            -1 => Box::new((1..self.height).rev()),
            _ => Box::new(0..self.height - 1),
        };
        let xrange: Box<dyn Iterator<Item = usize>> = match dir.x {
            -1 => Box::new((1..self.width).rev()),
            _ => Box::new(0..self.width - 1),
        };
        if dir.x == 0 {
            for y in yrange {
                for x in 0..self.width {
                    self.step_single(
                        x,
                        y,
                        (x as isize + dir.x) as usize,
                        (y as isize + dir.y) as usize,
                        &dir,
                    );
                }
            }
        } else {
            for x in xrange {
                for y in 0..self.height {
                    self.step_single(
                        x,
                        y,
                        (x as isize + dir.x) as usize,
                        (y as isize + dir.y) as usize,
                        &dir,
                    );
                }
            }
        }
    }

    fn step_single(&mut self, x: usize, y: usize, dx: usize, dy: usize, dir: &Vec2) {
        if self.is_sand(dx, dy) {
            let rand_choice = rand::random::<f32>();

            let ax = if dir.x == 0 { 1 } else { 0 };
            let ay = if dir.y == 0 { 1 } else { 0 };

            let check1 = if dir.x == 0 { x > 0 } else { y > 0 };
            let check2 = if dir.x == 0 {
                x < self.width - 1
            } else {
                y < self.height - 1
            };

            if self.is_empty(x, y) && !self.is_static(x, y) {
                self.swap(x, y, dx, dy);
            } else if rand_choice < 0.5 && check1 && self.is_empty(x - ax, y + ay) {
                self.swap(x - ax, y - ay, dx, dy);
            } else if rand_choice >= 0.5 && check2 && self.is_empty(x + ax, y + ay) {
                self.swap(x + ax, y + ay, dx, dy);
            }
        }
    }
}

fn get_keycode() -> Option<KeyCode> {
    if event::poll(std::time::Duration::from_millis(0)).ok()? {
        if let Ok(Event::Key(KeyEvent { code, .. })) = event::read() {
            return Some(code);
        }
    }
    None
}

fn main() {
    let mut args = Args::parse();
    if args.snow_chars.is_empty() {
        args.snow_chars = vec!['*', '.', '+'];
    }

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
        bg: u32::MAX,
        colors: std::collections::HashSet::new(),
        bg_colors: std::collections::HashSet::new(),
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
        for color in performer.bg_colors.iter() {
            write_bg_color(&mut lock, *color);
            write!(lock, "  ***** {}", color).unwrap();
            write!(lock, "\x1b[49m\n").unwrap();
        }
        return;
    }

    execute!(io::stdout(), EnterAlternateScreen, Hide, MoveTo(0, 0)).unwrap();
    enable_raw_mode().unwrap();
    performer.grid.render();
    if performer.grid.args.snow {
        loop {
            if let Some(keycode) = get_keycode() {
                match keycode {
                    KeyCode::Char(c) => {
                        if c == 'q' {
                            break;
                        }
                    }
                    KeyCode::Esc => break,
                    _ => {}
                }
            }
            let grid = &mut performer.grid;
            grid.snow_step();
            execute!(io::stdout(), MoveTo(0, 0), BeginSynchronizedUpdate).unwrap();
            performer.grid.render();
            execute!(io::stdout(), EndSynchronizedUpdate).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    } else {
        std::thread::sleep(std::time::Duration::from_millis(400));
        let cycles = performer.grid.args.blender;
        let antigravity = performer.grid.args.antigravity;
        let mut run_step = |dir: &Vec2| {
            let grid = &mut performer.grid;
            grid.step(&dir);
            execute!(io::stdout(), MoveTo(0, 0), BeginSynchronizedUpdate).unwrap();
            let grid = &performer.grid;
            grid.render();
            execute!(io::stdout(), EndSynchronizedUpdate).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(grid.args.ms));
        };

        // activate blender mode
        if cycles > 0 {
            let dirs = vec![
                Vec2 { x: -1, y: 0 },
                Vec2 { x: 0, y: -1 },
                Vec2 { x: 1, y: 0 },
                Vec2 { x: 0, y: 1 },
            ];
            for i in 0..(4 * cycles) {
                let dir = &dirs[i % 4];
                let iters = if dir.x == 0 { h * 2 / 3 } else { w * 2 / 3 };
                for _ in 0..iters {
                    run_step(&dir);
                }
            }
        } else {
            // normal sand sim
            let dir = if antigravity {
                Vec2 { x: 0, y: 1 }
            } else {
                Vec2 { x: 0, y: -1 }
            };
            for _ in 0..150 {
                run_step(&dir);
            }
        }
    }
    disable_raw_mode().unwrap();
    execute!(io::stdout(), LeaveAlternateScreen, Show).unwrap();
}
