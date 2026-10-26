// Copyright 2011 the V8 project authors. All rights reserved.
// Derived from Node v24.2.0 V8 date parser. See LICENSE-V8 and provenance.json.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    End,
    Unknown,
    Invalid,
    Space,
    Number,
    Symbol,
    Month,
    Zone,
    Separator,
    AmPm,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Token {
    pub(super) kind: Kind,
    pub(super) n: i32,
    pub(super) len: usize,
}
impl Token {
    pub(super) fn new(kind: Kind, n: i32, len: usize) -> Self {
        Self { kind, n, len }
    }
    pub(super) fn symbol(self, c: u8) -> bool {
        self.kind == Kind::Symbol && self.n == i32::from(c)
    }
    pub(super) fn number(self) -> bool {
        self.kind == Kind::Number
    }
    pub(super) fn fixed(self, n: usize) -> bool {
        self.number() && self.len == n
    }
    pub(super) fn sign(self) -> bool {
        self.symbol(b'+') || self.symbol(b'-')
    }
    pub(super) fn z(self) -> bool {
        self.kind == Kind::Zone && self.len == 1 && self.n == 0
    }
}
fn whitespace(c: u16) -> bool {
    matches!(
        c,
        9 | 11 | 12 | 32 | 160 | 0x1680 | 0x2000..=0x200a | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}
fn whitespace_line(c: u16) -> bool {
    whitespace(c) || matches!(c, 10 | 13 | 0x2028 | 0x2029)
}
pub(super) struct Scanner<'a> {
    input: &'a [u16],
    pos: usize,
    pub(super) look: Token,
}
impl<'a> Scanner<'a> {
    pub(super) fn new(input: &'a [u16]) -> Self {
        let mut s = Self {
            input,
            pos: 0,
            look: Token::new(Kind::End, 0, 0),
        };
        s.look = s.scan();
        s
    }
    fn ch(&self) -> u16 {
        self.input.get(self.pos).copied().unwrap_or(0)
    }
    pub(super) fn next(&mut self) -> Token {
        let t = self.look;
        self.look = self.scan();
        t
    }
    pub(super) fn skip(&mut self, c: u8) -> bool {
        if self.look.symbol(c) {
            self.next();
            true
        } else {
            false
        }
    }
    fn scan(&mut self) -> Token {
        let start = self.pos;
        let c = self.ch();
        if c == 0 {
            return Token::new(Kind::End, 0, 0);
        }
        if (48..=57).contains(&c) {
            while self.ch() == 48 {
                self.pos += 1;
            }
            let (mut n, mut digits) = (0, 0);
            while (48..=57).contains(&self.ch()) {
                if digits < 9 {
                    n = n * 10 + i32::from(self.ch() - 48);
                }
                digits += 1;
                self.pos += 1;
            }
            return Token::new(Kind::Number, n, self.pos - start);
        }
        if matches!(c, 58 | 45 | 43 | 46 | 41) {
            self.pos += 1;
            return Token::new(Kind::Symbol, i32::from(c), 1);
        }
        if c >= 65 && !whitespace(c) {
            let mut prefix = [0; 3];
            while self.ch() >= 65 && !whitespace(self.ch()) {
                let c = self.ch();
                if self.pos - start < 3 {
                    prefix[self.pos - start] = if (65..=90).contains(&c) { c + 32 } else { c }
                }
                self.pos += 1;
            }
            let len = self.pos - start;
            return keyword(prefix, len);
        }
        if whitespace_line(c) {
            self.pos += 1;
            return Token::new(Kind::Space, 0, 1);
        }
        if c == 40 {
            let mut depth = 0_usize;
            loop {
                let c = self.ch();
                if c == 40 {
                    depth += 1;
                } else if c == 41 {
                    depth -= 1;
                }
                self.pos += 1;
                if depth == 0 || self.ch() == 0 {
                    break;
                }
            }
            return Token::new(Kind::Unknown, 0, 1);
        }
        self.pos += 1;
        Token::new(Kind::Unknown, 0, 1)
    }
}

fn keyword(prefix: [u16; 3], len: usize) -> Token {
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    for (i, m) in months.iter().enumerate() {
        if prefix == [m.as_bytes()[0], m.as_bytes()[1], m.as_bytes()[2]].map(u16::from) {
            return Token::new(
                Kind::Month,
                i32::try_from(i).expect("twelve month entries") + 1,
                len,
            );
        }
    }
    for (name, kind, value) in [
        ("am", Kind::AmPm, 0),
        ("pm", Kind::AmPm, 12),
        ("ut", Kind::Zone, 0),
        ("utc", Kind::Zone, 0),
        ("z", Kind::Zone, 0),
        ("gmt", Kind::Zone, 0),
        ("cdt", Kind::Zone, -5),
        ("cst", Kind::Zone, -6),
        ("edt", Kind::Zone, -4),
        ("est", Kind::Zone, -5),
        ("mdt", Kind::Zone, -6),
        ("mst", Kind::Zone, -7),
        ("pdt", Kind::Zone, -7),
        ("pst", Kind::Zone, -8),
        ("t", Kind::Separator, 0),
    ] {
        let mut p = [0; 3];
        for (i, b) in name.bytes().enumerate() {
            p[i] = u16::from(b);
        }
        if prefix == p && len <= 3 {
            return Token::new(kind, value, len);
        }
    }
    Token::new(Kind::Invalid, 0, len)
}
