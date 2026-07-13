use anyhow::{bail, Context, Result};
use crossterm::{
    cursor::{Hide, MoveTo, MoveToNextLine, Show},
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, BeginSynchronizedUpdate, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use std::process::Command;
use tempfile::NamedTempFile;

use rand::seq::SliceRandom;
use rand::Rng;

use clap::Parser as ClapParser;
use clap::ValueEnum;
use unicode_width::UnicodeWidthChar;

use std::{
    io::{self, IsTerminal, Read, Write},
    u32,
};

use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};

#[allow(non_camel_case_types)]
#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
enum Effect {
    SNOW,
    GRAVITY,
    BLENDER,
    TILT_SHIFT,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
enum Direction {
    UP,
    DOWN,
    LEFT,
    RIGHT,
}

#[derive(ClapParser, Debug, Clone)]
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

    /// Make static characters "sticky," sand won't fall off directly (subtle)
    #[arg(short = 'e', long)]
    edge_stick: bool,

    /// Stickiness level, less results in short hills, more for tall towers.
    #[arg(short = 't', long, default_value_t = 0)]
    stickiness: usize,

    /// Which direction to apply gravity
    #[arg(value_enum, short = 'd', long, default_values_t = vec![Direction::DOWN])]
    direction: Vec<Direction>,

    /// How many times to repeat cycling effects (0 infinite loop: q or esc to quit)
    #[arg(long, default_value_t = 1)]
    cycles: usize,

    /// How many milliseconds to sleep for
    #[arg(long, default_value_t = 30)]
    ms: u64,

    /// How many simulation steps when simulating vertically
    #[arg(long, default_value_t = 150)]
    v_time: usize,

    /// How many simulation steps when simulating horizontally
    #[arg(long, default_value_t = 250)]
    h_time: usize,

    /// List of effects to enable
    #[arg(value_enum, num_args = 1.., value_delimiter = ' ', default_values_t = vec![Effect::GRAVITY])]
    effects: Vec<Effect>,
}

#[derive(Clone, Copy)]
struct Vec2 {
    x: isize,
    y: isize,
}

impl core::ops::Add<Vec2> for Vec2 {
    type Output = Self;

    fn add(self, rhs: Vec2) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl core::ops::Sub<Vec2> for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Vec2) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}
impl core::ops::Add<&Vec2> for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: &Vec2) -> Self::Output {
        Vec2 {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl core::ops::Sub<&Vec2> for Vec2 {
    type Output = Vec2;

    fn sub(self, rhs: &Vec2) -> Self::Output {
        Vec2 {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
enum Underline {
    #[default]
    Off = 0,
    Single = 1,
    Double = 2,
    Curly = 3,
    Dotted = 4,
    Dashed = 5,
}

#[derive(Default, Clone)]
struct CellStyle {
    italic: bool,
    bold: bool,
    strike: bool,
    underline: Underline,
}

/// This thing parses the initial input using anstyle-parse
struct Performer {
    grid: Grid,
    x: usize,
    y: usize,
    fg: u32,
    bg: u32,
    ul_color: u32,
    style: CellStyle,
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
        *self.grid.get_mut(self.x, self.y).unwrap() = Cell {
            c,
            fg: self.fg,
            bg: self.bg,
            ul_color: self.ul_color,
            style: self.style.clone(),
        };
        let mut width = UnicodeWidthChar::width(c).unwrap();
        while width > 0 {
            self.x += 1;
            if let Some(cell) = self.grid.get_mut(self.x, self.y) { cell.bg = self.bg }
            width -= 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        if byte == 0x0a {
            if self.x > 0 {
                self.grid.splat(self.x, self.y, self.bg);
            }
            self.y += 1;

            // self.bg = u32::MAX;
            self.x = 0;
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, _c: u8) {
        let items: Vec<_> = params.iter().collect();
        {
            match items[0][0] {
                0 => {
                    self.fg = u32::MAX;
                    self.bg = u32::MAX;
                    self.ul_color = u32::MAX;
                    self.colors.insert(self.fg);
                    self.style = CellStyle::default();
                }
                1 => {
                    self.style.bold = true;
                }
                3 => {
                    self.style.italic = true;
                }
                4 => {
                    self.ul_color = u32::MAX;
                    if items[0].len() == 1 {
                        self.style.underline = Underline::Single;
                    } else if items[0].len() == 2 {
                        self.style.underline = match items[0][1] {
                            0 => Underline::Off,
                            1 => Underline::Single,
                            2 => Underline::Double,
                            3 => Underline::Curly,
                            4 => Underline::Dotted,
                            5 => Underline::Dashed,
                            _ => Underline::Off,
                        };
                    }
                }
                9 => {
                    self.style.strike = true;
                }
                29 => {
                    self.style.strike = false;
                }
                24 => {
                    self.style.underline = Underline::Off;
                }
                22 => {
                    self.style.bold = false;
                }
                23 => {
                    self.style.italic = false;
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
                    self.fg = u32::MAX;
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
                58 => {
                    if items[1][0] == 5 {
                        self.ul_color = items[2][0] as u32;
                    } else if items[1][0] == 2 {
                        self.ul_color = (1 << 31)
                            ^ ((items[2][0] as u32) << 16)
                            ^ ((items[3][0] as u32) << 8)
                            ^ (items[4][0] as u32);
                    }
                }
                59 => {
                    self.ul_color = u32::MAX;
                }
                90..=97 => {
                    self.fg = (items[0][0] - 82) as u32;
                    self.colors.insert(self.fg);
                }
                100..=107 => {
                    self.bg = (items[0][0] - 92) as u32;
                    self.bg_colors.insert(self.bg);
                }
                _ => {
                    // println!(
                    //     "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
                    //     params, intermediates, ignore, c
                    // );
                }
            }
        }
    }
}

#[derive(Clone)]
struct Cell {
    fg: u32,
    bg: u32,
    ul_color: u32,
    c: char,
    style: CellStyle,
}
struct Grid {
    width: usize,
    height: usize,
    data: Box<[Cell]>,
    args: Args,
    stickiness: f32,
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

impl Cell {
    fn is_box_char(&self) -> bool {
        match self.c {
            '\u{2500}'..='\u{257F}' => true,
            _ => false,
        }
    }
    fn is_empty(&self) -> bool {
        self.c == '\0' || self.c == ' '
    }
}

fn write_bg_color(lock: &mut io::StdoutLock<'static>, bg: u32) {
    if bg == u32::MAX {
        write!(lock, "\x1b[49m").unwrap();
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
    if fg == u32::MAX {
        write!(lock, "\x1b[39m").unwrap();
    } else if fg < (1 << 31) {
        write!(lock, "\x1b[38;5;{}m", fg).unwrap();
    } else {
        let r = ((fg >> 16) & 0xFF) as u8;
        let g = ((fg >> 8) & 0xFF) as u8;
        let b = ((fg) & 0xFF) as u8;
        write!(lock, "\x1b[38;2;{};{};{}m", r, g, b).unwrap();
    }
}
fn write_color_escaped(lock: &mut io::StdoutLock<'static>, fg: u32) {
    if fg == u32::MAX {
        write!(lock, "\\x1b[39m").unwrap();
    } else if fg < (1 << 31) {
        write!(lock, "\\x1b[38;5;{}m", fg).unwrap();
    } else {
        let r = ((fg >> 16) & 0xFF) as u8;
        let g = ((fg >> 8) & 0xFF) as u8;
        let b = ((fg) & 0xFF) as u8;
        write!(lock, "\\x1b[38;2;{};{};{}m", r, g, b).unwrap();
    }
}
fn write_ul_color(lock: &mut io::StdoutLock<'static>, ul: u32) {
    if ul == u32::MAX {
        write!(lock, "\x1b[59m").unwrap();
    } else if ul < (1 << 31) {
        write!(lock, "\x1b[58;5;{}m", ul).unwrap();
    } else {
        let r = ((ul >> 16) & 0xFF) as u8;
        let g = ((ul >> 8) & 0xFF) as u8;
        let b = ((ul) & 0xFF) as u8;
        write!(lock, "\x1b[58;2;{};{};{}m", r, g, b).unwrap();
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
    fn step(&mut self, args: &Args, _dir: &Vec2) {
        self.flip = (self.flip + 1) % self.flip_rate;
        if self.flip != 0 {
            return;
        }
        // spawn
        let mut rng = rand::thread_rng();
        let rand_x = rng.gen_range(0..self.width);
        self.get_mut(rand_x, 0).c = *args.snow_chars.choose(&mut rng).unwrap();
        for y in (1..self.height).rev() {
            let delta = y - 1;
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
        false
    }
    fn swap(&mut self, x1: usize, y1: usize, x2: usize, y2: usize) {
        let idx1 = y1 * self.width + x1;
        let idx2 = y2 * self.width + x2;

        self.data.swap(idx1, idx2);
    }
}
impl Grid {
    fn new(args: Args, w: usize, h: usize) -> Self {
        let stickiness = 0.5 / (args.stickiness as f32).max(1.0);
        Grid {
            args,
            width: w,
            height: h,
            stickiness,
            data: vec![
                Cell {
                    fg: u32::MAX,
                    bg: u32::MAX,
                    ul_color: u32::MAX,
                    style: CellStyle::default(),
                    c: ' ',
                };
                w * h
            ]
            .into_boxed_slice(),
            snow_fg: SnowGrid {
                width: w,
                height: h,
                data: vec![Snowflake { c: ' ' }; w * h].into_boxed_slice(),
                flip: 0,
                flip_rate: 8,
            },
            snow_bg: SnowGrid {
                width: w,
                height: h,
                data: vec![Snowflake { c: ' ' }; w * h].into_boxed_slice(),
                flip: 0,
                flip_rate: 24,
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
            self.get_mut(i, y).unwrap().bg = bg;
            i += 1;
        }
    }
    fn get(&self, pos: Vec2) -> Option<&Cell> {
        if pos.x < 0 || pos.x >= self.width as isize || pos.y < 0 || pos.y >= self.height as isize {
            return None;
        }
        Some(&self.data[pos.y as usize * self.width + pos.x as usize])
    }
    fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        if x < 0 || x >= self.width || y < 0 || y >= self.height {
            return None;
        }
        Some(&mut self.data[y * self.width + x])
    }

    fn swap(&mut self, pos1: Vec2, pos2: Vec2) {
        let idx1 = pos1.y as usize * self.width + pos1.x as usize;
        let idx2 = pos2.y as usize * self.width + pos2.x as usize;

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
    fn is_static(&self, cell: &Cell) -> bool {
        (self.args.borders && cell.is_box_char())
            || (!cell.is_empty()
                && (self.args.color.contains(&cell.fg) || self.args.bg.contains(&cell.bg)))

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

    fn is_sand(&self, cell: &Cell) -> bool {
        !cell.is_empty() && !self.is_static(cell)
    }

    fn render(&self) {
        let mut fg = u32::MAX;
        let mut bg = u32::MAX;
        let mut ul = u32::MAX;
        let mut style = CellStyle::default();
        let mut lock = io::stdout().lock();
        write!(lock, "\x1b[0m").unwrap();
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
                if style.italic != d.style.italic {
                    style.italic = d.style.italic;
                    if style.italic {
                        write!(lock, "\x1b[3m").unwrap();
                    } else {
                        write!(lock, "\x1b[23m").unwrap();
                    }
                }
                if style.strike != d.style.strike {
                    style.strike = d.style.strike;
                    if style.strike {
                        write!(lock, "\x1b[9m").unwrap();
                    } else {
                        write!(lock, "\x1b[29m").unwrap();
                    }
                }
                if style.bold != d.style.bold {
                    style.bold = d.style.bold;
                    if style.bold {
                        write!(lock, "\x1b[1m").unwrap();
                    } else {
                        write!(lock, "\x1b[22m").unwrap();
                    }
                }
                if style.underline != d.style.underline {
                    style.underline = d.style.underline;
                    write!(lock, "\x1b[4:{}m", style.underline as u32).unwrap();
                    ul = u32::MAX;
                }
                if let Underline::Off = d.style.underline {
                } else if ul != d.ul_color {
                    write_ul_color(&mut lock, d.ul_color);
                    ul = d.ul_color;
                }
                if snow_fg.is_some() || (snow_bg.is_some() && d.c == ' ') {
                    if fg != 15 {
                        fg = 15;
                        write_color(&mut lock, fg);
                    }
                } else if fg != d.fg {
                    fg = d.fg;
                    write_color(&mut lock, fg);
                }
                if d.c == '\0' && y < self.height - 1 {
                    execute!(lock, MoveToNextLine(1)).unwrap();
                    break;
                }
                if bg != d.bg {
                    bg = d.bg;
                    write_bg_color(&mut lock, bg);
                }
                if let Some(snow_fg) = snow_fg {
                    write!(lock, "{}", snow_fg).unwrap();
                } else {
                    if d.c == ' ' && snow_bg.is_some() {
                        write!(lock, "{}", snow_bg.unwrap()).unwrap();
                    } else {
                        let width = UnicodeWidthChar::width(d.c);
                        if let Some(width) = width {
                            if width > 1 {
                                skip = width - 1;
                            }
                            if x + width <= self.width {
                                write!(lock, "{}", d.c).unwrap();
                            } else {
                                write!(lock, " ").unwrap();
                            }
                        } else {
                            write!(lock, "{}", d.c).unwrap();
                        }
                    }
                }
            }
        }
        execute!(lock, MoveTo(0, 0)).unwrap();
    }

    fn snow_step(&mut self, dir: &Vec2) {
        self.snow_fg.step(&self.args, dir);
        self.snow_bg.step(&self.args, dir);
    }

    fn step(&mut self, dir: &Vec2) {
        let yrange: Box<dyn Iterator<Item = usize>> = match dir.y {
            -1 => Box::new((1..self.height).rev()), // down (process from bottom up)
            _ => Box::new(0..self.height - 1),      // up
        };
        let xrange: Box<dyn Iterator<Item = usize>> = match dir.x {
            -1 => Box::new((1..self.width).rev()),
            _ => Box::new(0..self.width - 1),
        };
        if dir.x == 0 {
            for y in yrange {
                for x in 0..self.width {
                    let pos = Vec2 {
                        x: x as isize,
                        y: y as isize,
                    };
                    let up = pos + dir;
                    let dir_right = Vec2 {
                        x: -dir.y,
                        y: dir.x,
                    };
                    self.step_collapse(pos, up, dir_right);
                    self.step_fall(pos, up);
                }
                // for x in 0..self.width {
                //     let pos = Vec2 { x: x as isize, y: y as isize };
                //     let up = pos + dir;
                //     let dir_right = Vec2 {
                //         x: -dir.y,
                //         y: dir.x,
                //     };
                //     self.step_slide(pos, up, dir_right);
                // }
            }
        } else {
            for x in xrange {
                // everything that can fall, will fall
                for y in 0..self.height {
                    let pos = Vec2 {
                        x: x as isize,
                        y: y as isize,
                    };
                    let up = pos + dir;
                    let dir_right = Vec2 {
                        x: -dir.y,
                        y: dir.x,
                    };
                    self.step_collapse(pos, up, dir_right);
                    self.step_fall(pos, up);
                }
                // everything that might slide, should slide
                // for y in 0..self.height {
                //     let pos = Vec2 { x: x as isize, y: y as isize };
                //     let up = pos + dir;
                //     let dir_right = Vec2 {
                //         x: -dir.y,
                //         y: dir.x,
                //     };
                //     self.step_slide(pos, up, dir_right);
                // }
            }
        }
    }

    fn step_fall(&mut self, pos: Vec2, up: Vec2) {
        let Some(current) = self.get(pos) else { return };
        let Some(cell_up) = self.get(up) else { return };
        if !self.is_sand(cell_up) {
            return;
        }
        if current.is_empty() && !self.is_static(cell_up) {
            self.swap(pos, up);
        }
    }
    fn step_slide(&mut self, pos: Vec2, up: Vec2, dir_right: Vec2) {
        let Some(_current) = self.get(pos) else { return };
        let Some(up_cell) = self.get(up) else { return };
        if !self.is_sand(up_cell) {
            return;
        }
        let rand_choice = rand::random::<f32>();

        let left = pos - dir_right;
        let right = pos + dir_right;
        let cell_left = self.get(left);
        let cell_right = self.get(right);
        let left_valid = if let Some(cell_left) = cell_left {
            cell_left.is_empty() && !self.is_static(cell_left)
        } else {
            false
        };
        let right_valid = if let Some(cell_right) = cell_right {
            cell_right.is_empty() && !self.is_static(cell_right)
        } else {
            false
        };
        // do the swaps
        if left_valid {
            if right_valid && rand_choice > 0.5 {
                self.swap(right, up);
            } else {
                self.swap(left, up);
            }
        } else if right_valid {
            self.swap(right, up);
        }
    }
    fn step_collapse(&mut self, pos: Vec2, up: Vec2, dir_right: Vec2) {
        let Some(current) = self.get(pos) else { return };
        let Some(up_cell) = self.get(up) else { return };
        if !current.is_empty() {
            return;
        }
        let rand_choice = rand::random::<f32>();

        let cell_left = self.get(pos - dir_right);
        let cell_right = self.get(pos + dir_right);
        let cell_up_left = self.get(up - dir_right);
        let cell_up_right = self.get(up + dir_right);
        let left_valid = if let (Some(cell_left), Some(cell_up_left)) = (cell_left, cell_up_left) {
            !cell_left.is_empty()
                && !self.is_static(up_cell)
                && (!self.args.edge_stick || !self.is_static(cell_left))
                && self.is_sand(cell_up_left)
        } else {
            false
        };
        let right_valid =
            if let (Some(cell_right), Some(cell_up_right)) = (cell_right, cell_up_right) {
                !cell_right.is_empty()
                    && !self.is_static(up_cell)
                    && (!self.args.edge_stick || !self.is_static(cell_right))
                    && self.is_sand(cell_up_right)
            } else {
                false
            };
        // do the swaps
        if left_valid {
            if right_valid && rand_choice > (1.0 - self.stickiness) {
                self.swap(up + dir_right, pos);
            } else if rand_choice < self.stickiness {
                self.swap(up - dir_right, pos);
            }
        } else if right_valid && rand_choice > (1.0 - self.stickiness) {
            self.swap(up + dir_right, pos);
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

fn check_quit() -> bool {
    let Some(keycode) = get_keycode() else {
        return false;
    };
    match keycode {
        KeyCode::Char('q') => true,
        KeyCode::Esc => true,
        _ => false,
    }
}

fn fork_self_helper(args: &Args) -> Result<()> {
    let output = Command::new("tmux").arg("display").arg("-p").arg("#{pane_id} #{pane_width} #{pane_height} #{pane_left} #{pane_top} #{status}-#{status-position}").output().context("failed to spawn tmux display")?;
    if !output.status.success() {
        bail!("failed to read tmux state");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<_> = stdout.split(" ").collect();
    if parts.len() != 6 {
        bail!("where did you find this tmux");
    }

    let pane = parts[0];
    let width = parts[1].parse::<i32>()?;
    let height = parts[2].parse::<i32>()?;
    let left = parts[3].parse::<i32>()?;
    let top = parts[4].parse::<i32>()?;
    let mut y = top + height;
    if parts[5] == "on-top" {
        y += 1;
    }
    let capture = Command::new("tmux")
        .arg("capture-pane")
        .arg("-CTpet")
        .arg(pane)
        .output()
        .context("failed to spawn tmux capture-pane")?;
    if !capture.status.success() {
        bail!("failed to capture");
    }
    let stdout = String::from_utf8_lossy(&capture.stdout);
    let mut file = NamedTempFile::new()?;
    writeln!(file, "{}", stdout.replace("\\033", "\x1b"))?;
    let path = file.into_temp_path();
    let raw_args: Vec<String> = std::env::args().collect();
    let self_arg = if raw_args[0].starts_with("./") {
        let canon = std::fs::canonicalize(&raw_args[0])?;
        canon.to_owned().to_string_lossy().to_string()
    } else {
        match which::which(&raw_args[0]) {
            Ok(path) => path.to_owned().to_string_lossy().to_string(),
            Err(_) => "termsand".to_string(),
        }
    };
    let args_without_first = &raw_args[1..];
    let termsand_invocation = format!(
        "TERMSAND_DONT_FORK=1 {} {} < {}",
        self_arg,
        shell_words::join(args_without_first),
        path.to_str().unwrap()
    );
    let mut cmd = Command::new("tmux");
    cmd.arg("display-popup").arg("-B");

    if !args.list_colors {
        cmd.arg("-E");
    }

    let popup = cmd
        .arg("-y")
        .arg(y.to_string())
        .arg("-x")
        .arg(left.to_string())
        .arg("-w")
        .arg(width.to_string())
        .arg("-h")
        .arg(height.to_string())
        .arg(termsand_invocation)
        .output()
        .context("failed to spawn tmux display-popup")?;
    if !popup.status.success() {
        bail!("failed to spawn popup");
    }
    if args.list_colors {
        println!("{}", String::from_utf8_lossy(&popup.stdout));
    }
    Ok(())
}
fn fork_self(args: Args) {
    if std::env::var("TERMSAND_DONT_FORK").is_ok() {
        std::process::exit(1);
    }
    let result = fork_self_helper(&args);
    if let Err(e) = result {
        panic!("error: {:?}", e);
    };
}

fn main() {
    let mut args = Args::parse();
    if args.snow_chars.is_empty() {
        args.snow_chars = vec!['*', '.', '+'];
    }

    let stdin = io::stdin();
    let is_terminal = stdin.is_terminal();

    if is_terminal {
        return fork_self(args);
    }
    let Some((w, h)) = term_size::dimensions() else {
        return fork_self(args);
    };
    let input = io::stdin();
    let mut handle = input.lock();

    let mut statemachine = Parser::<DefaultCharAccumulator>::new();
    let mut performer = Performer {
        grid: Grid::new(args.clone(), w, h),
        x: 0,
        y: 0,
        fg: u32::MAX,
        bg: u32::MAX,
        ul_color: u32::MAX,
        style: CellStyle::default(),
        colors: std::collections::HashSet::new(),
        bg_colors: std::collections::HashSet::new(),
    };

    let mut buf = [0; 2048];

    let mut bytes_read = 0;
    loop {
        match handle.read(&mut buf) {
            Ok(0) => {
                if bytes_read == 0 {
                    return fork_self(args);
                }
                break;
            }
            Ok(n) => {
                for byte in &buf[..n] {
                    statemachine.advance(&mut performer, *byte);
                }
                bytes_read += n;
            }
            Err(_err) => {
                break;
            }
        }
    }

    if performer.grid.args.list_colors {
        let mut lock = io::stdout().lock();
        writeln!(
            lock,
            "These numbers can be used for the '--color' and '--bg' flags."
        )
        .unwrap();
        writeln!(lock, "Colors detected in input:").unwrap();
        for color in performer.colors.iter() {
            write_color(&mut lock, *color);
            write!(lock, "  ***** {} ", color).unwrap();
            write!(lock, "\x1b[39m").unwrap();
            // write_color_escaped(&mut lock, *color);
            writeln!(lock).unwrap();
        }
        writeln!(lock, "\x1b[39m").unwrap();
        for color in performer.bg_colors.iter() {
            write_bg_color(&mut lock, *color);
            write!(lock, "  ***** {}", color).unwrap();
            writeln!(lock, "\x1b[49m").unwrap();
        }
        return;
    }

    execute!(io::stdout(), EnterAlternateScreen, Hide, MoveTo(0, 0)).unwrap();
    enable_raw_mode().unwrap();
    'done: {
        let snow = performer.grid.args.effects.contains(&Effect::SNOW);

        // TODO: is there a better way to write this?
        let gravity = performer.grid.args.effects.contains(&Effect::GRAVITY)
            || performer.grid.args.effects.contains(&Effect::TILT_SHIFT)
            || performer.grid.args.effects.contains(&Effect::BLENDER);
        performer.grid.render();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let Args {
            mut cycles,
            h_time,
            v_time,
            ..
        } = performer.grid.args;
        let user_dirs = &performer
            .grid
            .args
            .direction
            .iter()
            .map(|d| match d {
                Direction::UP => Vec2 { x: 0, y: 1 },
                Direction::DOWN => Vec2 { x: 0, y: -1 },
                Direction::LEFT => Vec2 { x: 1, y: 0 },
                Direction::RIGHT => Vec2 { x: -1, y: 0 },
            })
            .collect::<Vec<_>>();
        let effects = performer.grid.args.effects.clone();
        let mut funny_modulus_thing = 0;
        let mut run_step = |dir: &Vec2| {
            let grid = &mut performer.grid;
            if snow {
                grid.snow_step(dir);
            }
            if gravity {
                grid.step(dir);
            }
            if grid.args.ms == 0 {
                funny_modulus_thing += 1;
                if funny_modulus_thing < 5 {
                    return;
                }
                funny_modulus_thing = 0;
            }
            execute!(io::stdout(), MoveTo(0, 0), BeginSynchronizedUpdate).unwrap();
            let grid = &performer.grid;
            grid.render();
            execute!(io::stdout(), EndSynchronizedUpdate).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(grid.args.ms));
        };
        let dirs: &[Vec2];
        // activate blender mode
        if effects.contains(&Effect::BLENDER) {
            dirs = &[
                Vec2 { x: -1, y: 0 },
                Vec2 { x: 0, y: -1 },
                Vec2 { x: 1, y: 0 },
                Vec2 { x: 0, y: 1 },
            ];
        } else if effects.contains(&Effect::TILT_SHIFT) {
            dirs = &[
                Vec2 { x: -1, y: 0 }, // right
                Vec2 { x: 1, y: 0 },  // left
                Vec2 { x: 0, y: 1 },  // up
                Vec2 { x: 0, y: -1 }, // down
                Vec2 { x: -1, y: 0 }, // right
                Vec2 { x: 0, y: 1 },  // up
                Vec2 { x: -1, y: 0 }, // down
                Vec2 { x: 1, y: 0 },  // left
            ];
        } else {
            // normal sand sim
            dirs = user_dirs;
            if cycles == 0 && gravity {
                cycles = 1;
            }
        }
        if cycles == 0 {
            for dir in dirs.iter().cycle() {
                let iters = if dir.x == 0 { v_time } else { h_time };
                for _ in 0..iters {
                    if check_quit() {
                        break 'done;
                    }
                    run_step(dir);
                }
            }
        } else {
            for dir in dirs.iter().cycle().take(dirs.len() * cycles) {
                let iters = if dir.x == 0 { v_time } else { h_time };
                for _ in 0..iters {
                    if check_quit() {
                        break 'done;
                    }
                    run_step(dir);
                }
            }
        }
    }
    disable_raw_mode().unwrap();
    execute!(io::stdout(), LeaveAlternateScreen, Show).unwrap();
}
