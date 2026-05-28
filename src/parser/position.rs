/// A location in the source, tracked as the parser consumes bytes. Lines and
/// columns are 1-based; `byte` is a 0-based offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new() -> Self {
        Self {
            byte: 0,
            line: 1,
            col: 1,
        }
    }
    pub fn advance(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte += 1;
            if b == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {} (byte {})",
            self.line, self.col, self.byte
        )
    }
}
