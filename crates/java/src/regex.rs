//! `java.util.regex`: a small backtracking engine and the `Pattern`/`Matcher` natives.
//!
//! Supported syntax: literals, `.`, character classes (`[...]`, ranges, negation, `\d \D \w \W
//! \s \S`), anchors (`^ $`), word boundaries, groups (`(...)`, `(?:...)`), alternation, and the
//! quantifiers `* + ? {n} {n,} {n,m}` with an optional lazy `?`. Backreferences and lookaround are
//! rejected at compile time. A step budget bounds backtracking so pathological patterns fail
//! instead of hanging.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use oxjvm_classfile::flags::*;
use oxjvm_vm::{ObjectRef, Value, Vm, VmError};

use crate::{boolean, class, field, int, method, object, receiver, ref_arg, string_arg, void};

// -------------------------------------------------------------------------------------------
// Engine
// -------------------------------------------------------------------------------------------

pub(crate) const FLAG_CASE_INSENSITIVE: i32 = 0x02;
pub(crate) const FLAG_MULTILINE: i32 = 0x08;
pub(crate) const FLAG_LITERAL: i32 = 0x10;
pub(crate) const FLAG_DOTALL: i32 = 0x20;

/// Bound on backtracking work per attempted match, so bad patterns fail fast.
const STEP_LIMIT: usize = 500_000;

#[derive(Clone, Debug)]
enum ClassItem {
    Char(u16),
    Range(u16, u16),
    Digit(bool),
    Word(bool),
    Space(bool),
}

#[derive(Clone, Debug, Default)]
struct ClassSet {
    negated: bool,
    items: Vec<ClassItem>,
}

impl ClassSet {
    fn matches(&self, ch: u16, case_insensitive: bool) -> bool {
        let hit = self.items.iter().any(|item| match item {
            ClassItem::Char(c) => eq_char(ch, *c, case_insensitive),
            ClassItem::Range(a, b) => {
                let (ch, a, b) = if case_insensitive {
                    (ascii_lower(ch), ascii_lower(*a), ascii_lower(*b))
                } else {
                    (ch, *a, *b)
                };
                ch >= a && ch <= b
            }
            ClassItem::Digit(positive) => is_digit(ch) == *positive,
            ClassItem::Word(positive) => is_word(ch) == *positive,
            ClassItem::Space(positive) => is_space(ch) == *positive,
        });
        hit != self.negated
    }
}

#[derive(Clone, Debug)]
enum Node {
    Empty,
    Char(u16),
    Any,
    Class(ClassSet),
    Start,
    End,
    WordBoundary(bool),
    Group(usize, Box<Node>),
    Concat(Vec<Node>),
    Alternate(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: usize,
        max: Option<usize>,
        lazy: bool,
    },
}

#[derive(Clone, Debug)]
enum Inst {
    Char(u16),
    Any,
    Class(ClassSet),
    Start,
    End,
    WordBoundary(bool),
    SaveStart(usize),
    SaveEnd(usize),
    Split(usize, usize),
    Jmp(usize),
    Match,
}

/// A compiled pattern.
pub(crate) struct Regex {
    prog: Vec<Inst>,
    groups: usize,
    case_insensitive: bool,
    multiline: bool,
    dotall: bool,
}

/// One match with its capture spans (UTF-16 offsets).
pub(crate) struct RegexMatch {
    pub start: usize,
    pub end: usize,
    pub groups: Vec<Option<(usize, usize)>>,
}

fn ascii_lower(ch: u16) -> u16 {
    if (65..=90).contains(&ch) { ch + 32 } else { ch }
}

fn eq_char(a: u16, b: u16, case_insensitive: bool) -> bool {
    a == b || (case_insensitive && ascii_lower(a) == ascii_lower(b))
}

fn is_digit(ch: u16) -> bool {
    ch < 128 && (ch as u8).is_ascii_digit()
}

fn is_word(ch: u16) -> bool {
    ch < 128 && ((ch as u8).is_ascii_alphanumeric() || ch == b'_' as u16)
}

fn is_space(ch: u16) -> bool {
    matches!(ch, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D)
}

struct Parser<'a> {
    pattern: &'a [u16],
    pos: usize,
    groups: usize,
}

type ParseError = (String, usize);

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u16> {
        self.pattern.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<u16> {
        let ch = self.peek();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn parse(mut self) -> Result<(Node, usize), ParseError> {
        let node = self.parse_alternation()?;
        if self.pos < self.pattern.len() {
            return Err(("Unmatched closing ')'".into(), self.pos));
        }
        Ok((node, self.groups))
    }

    fn parse_alternation(&mut self) -> Result<Node, ParseError> {
        let mut branches = vec![self.parse_concat()?];
        while self.peek() == Some(b'|' as u16) {
            self.pos += 1;
            branches.push(self.parse_concat()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().expect("one branch")
        } else {
            Node::Alternate(branches)
        })
    }

    fn parse_concat(&mut self) -> Result<Node, ParseError> {
        let mut nodes = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == b'|' as u16 || ch == b')' as u16 {
                break;
            }
            nodes.push(self.parse_repeat()?);
        }
        Ok(match nodes.len() {
            0 => Node::Empty,
            1 => nodes.pop().expect("one node"),
            _ => Node::Concat(nodes),
        })
    }

    fn parse_repeat(&mut self) -> Result<Node, ParseError> {
        let atom = self.parse_atom()?;
        let (min, max) = match self.peek() {
            Some(ch) if ch == b'*' as u16 => {
                self.pos += 1;
                (0, None)
            }
            Some(ch) if ch == b'+' as u16 => {
                self.pos += 1;
                (1, None)
            }
            Some(ch) if ch == b'?' as u16 => {
                self.pos += 1;
                (0, Some(1))
            }
            Some(ch) if ch == b'{' as u16 => {
                let saved = self.pos;
                self.pos += 1;
                let min = self.parse_number();
                let (min, max) = match (min, self.peek()) {
                    (Some(min), Some(ch)) if ch == b',' as u16 => {
                        self.pos += 1;
                        let max = self.parse_number();
                        (min, max)
                    }
                    (Some(min), Some(ch)) if ch == b'}' as u16 => {
                        self.pos += 1;
                        (min, Some(min))
                    }
                    _ => {
                        self.pos = saved;
                        return Ok(atom);
                    }
                };
                match self.peek() {
                    Some(ch) if ch == b'}' as u16 => {
                        self.pos += 1;
                    }
                    _ => {
                        self.pos = saved;
                        return Ok(atom);
                    }
                }
                (min, max)
            }
            _ => return Ok(atom),
        };
        let lazy = if self.peek() == Some(b'?' as u16) {
            self.pos += 1;
            true
        } else {
            false
        };
        if let Some(max) = max {
            if max < min {
                return Err(("Illegal repetition range".into(), self.pos));
            }
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            lazy,
        })
    }

    fn parse_number(&mut self) -> Option<usize> {
        let mut digits = String::new();
        while let Some(ch) = self.peek() {
            if (b'0' as u16..=b'9' as u16).contains(&ch) {
                digits.push((ch as u8) as char);
                self.pos += 1;
            } else {
                break;
            }
        }
        digits.parse().ok()
    }

    fn parse_atom(&mut self) -> Result<Node, ParseError> {
        let Some(ch) = self.next() else {
            return Err(("Unexpected end of pattern".into(), self.pos));
        };
        match ch {
            _ if ch == b'(' as u16 => {
                if self.peek() == Some(b'?' as u16) {
                    self.pos += 1;
                    match self.next() {
                        Some(colon) if colon == b':' as u16 => {
                            let inner = self.parse_alternation()?;
                            self.expect_close()?;
                            Ok(inner)
                        }
                        _ => Err(("Unsupported group construct".into(), self.pos)),
                    }
                } else {
                    self.groups += 1;
                    let index = self.groups - 1;
                    let inner = self.parse_alternation()?;
                    self.expect_close()?;
                    Ok(Node::Group(index, Box::new(inner)))
                }
            }
            _ if ch == b'[' as u16 => self.parse_class(),
            _ if ch == b'.' as u16 => Ok(Node::Any),
            _ if ch == b'^' as u16 => Ok(Node::Start),
            _ if ch == b'$' as u16 => Ok(Node::End),
            _ if ch == b'\\' as u16 => self.parse_escape(),
            _ if ch == b'*' as u16 || ch == b'+' as u16 || ch == b'?' as u16 => {
                Err(("Dangling quantifier".into(), self.pos - 1))
            }
            _ => Ok(Node::Char(ch)),
        }
    }

    fn expect_close(&mut self) -> Result<(), ParseError> {
        if self.peek() == Some(b')' as u16) {
            self.pos += 1;
            Ok(())
        } else {
            Err(("Unclosed group".into(), self.pos))
        }
    }

    fn parse_escape(&mut self) -> Result<Node, ParseError> {
        let Some(ch) = self.next() else {
            return Err(("Unexpected end of pattern".into(), self.pos));
        };
        match ch {
            _ if ch == b'd' as u16 => Ok(Node::Class(single_class(ClassItem::Digit(true)))),
            _ if ch == b'D' as u16 => Ok(Node::Class(single_class(ClassItem::Digit(false)))),
            _ if ch == b'w' as u16 => Ok(Node::Class(single_class(ClassItem::Word(true)))),
            _ if ch == b'W' as u16 => Ok(Node::Class(single_class(ClassItem::Word(false)))),
            _ if ch == b's' as u16 => Ok(Node::Class(single_class(ClassItem::Space(true)))),
            _ if ch == b'S' as u16 => Ok(Node::Class(single_class(ClassItem::Space(false)))),
            _ if ch == b'b' as u16 => Ok(Node::WordBoundary(true)),
            _ if ch == b'B' as u16 => Ok(Node::WordBoundary(false)),
            _ if ch == b'A' as u16 => Ok(Node::Start),
            _ if ch == b'z' as u16 || ch == b'Z' as u16 => Ok(Node::End),
            _ if ch == b'n' as u16 => Ok(Node::Char(0x0A)),
            _ if ch == b'r' as u16 => Ok(Node::Char(0x0D)),
            _ if ch == b't' as u16 => Ok(Node::Char(0x09)),
            _ if ch == b'f' as u16 => Ok(Node::Char(0x0C)),
            _ if (b'1' as u16..=b'9' as u16).contains(&ch) => {
                Err(("Backreferences are not supported".into(), self.pos - 1))
            }
            _ => Ok(Node::Char(ch)),
        }
    }

    fn parse_class(&mut self) -> Result<Node, ParseError> {
        let mut set = ClassSet::default();
        if self.peek() == Some(b'^' as u16) {
            set.negated = true;
            self.pos += 1;
        }
        let mut first = true;
        loop {
            if self.peek().is_none() {
                return Err(("Unclosed character class".into(), self.pos));
            }
            if !first && self.peek() == Some(b']' as u16) {
                self.pos += 1;
                return Ok(Node::Class(set));
            }
            let start = self.class_atom()?;
            match start {
                ClassAtom::Special(item) => set.items.push(item),
                ClassAtom::Single(ch) => {
                    let ranges = self.peek() == Some(b'-' as u16)
                        && self.pos + 1 < self.pattern.len()
                        && self.pattern[self.pos + 1] != b']' as u16;
                    if ranges {
                        self.pos += 1;
                        match self.class_atom()? {
                            ClassAtom::Single(end) => set.items.push(ClassItem::Range(ch, end)),
                            ClassAtom::Special(item) => {
                                set.items.push(ClassItem::Char(ch));
                                set.items.push(ClassItem::Char(b'-' as u16));
                                set.items.push(item);
                            }
                        }
                    } else {
                        set.items.push(ClassItem::Char(ch));
                    }
                }
            }
            first = false;
        }
    }

    fn class_atom(&mut self) -> Result<ClassAtom, ParseError> {
        let Some(ch) = self.next() else {
            return Err(("Unclosed character class".into(), self.pos));
        };
        if ch != b'\\' as u16 {
            return Ok(ClassAtom::Single(ch));
        }
        let Some(escape) = self.next() else {
            return Err(("Unexpected end of pattern".into(), self.pos));
        };
        Ok(match escape {
            _ if escape == b'd' as u16 => ClassAtom::Special(ClassItem::Digit(true)),
            _ if escape == b'D' as u16 => ClassAtom::Special(ClassItem::Digit(false)),
            _ if escape == b'w' as u16 => ClassAtom::Special(ClassItem::Word(true)),
            _ if escape == b'W' as u16 => ClassAtom::Special(ClassItem::Word(false)),
            _ if escape == b's' as u16 => ClassAtom::Special(ClassItem::Space(true)),
            _ if escape == b'S' as u16 => ClassAtom::Special(ClassItem::Space(false)),
            _ if escape == b'n' as u16 => ClassAtom::Single(0x0A),
            _ if escape == b'r' as u16 => ClassAtom::Single(0x0D),
            _ if escape == b't' as u16 => ClassAtom::Single(0x09),
            _ if escape == b'f' as u16 => ClassAtom::Single(0x0C),
            _ => ClassAtom::Single(escape),
        })
    }
}

enum ClassAtom {
    Single(u16),
    Special(ClassItem),
}

fn single_class(item: ClassItem) -> ClassSet {
    ClassSet {
        negated: false,
        items: vec![item],
    }
}

fn compile_node(node: &Node, prog: &mut Vec<Inst>, case_insensitive: bool) {
    match node {
        Node::Empty => {}
        Node::Char(ch) => prog.push(Inst::Char(*ch)),
        Node::Any => prog.push(Inst::Any),
        Node::Class(set) => prog.push(Inst::Class(set.clone())),
        Node::Start => prog.push(Inst::Start),
        Node::End => prog.push(Inst::End),
        Node::WordBoundary(positive) => prog.push(Inst::WordBoundary(*positive)),
        Node::Group(index, inner) => {
            prog.push(Inst::SaveStart(*index));
            compile_node(inner, prog, case_insensitive);
            prog.push(Inst::SaveEnd(*index));
        }
        Node::Concat(nodes) => {
            for child in nodes {
                compile_node(child, prog, case_insensitive);
            }
        }
        Node::Alternate(branches) => {
            let mut jumps = Vec::new();
            for (index, branch) in branches.iter().enumerate() {
                if index + 1 == branches.len() {
                    compile_node(branch, prog, case_insensitive);
                    break;
                }
                let split = prog.len();
                prog.push(Inst::Split(0, 0));
                let body = prog.len();
                compile_node(branch, prog, case_insensitive);
                let jump = prog.len();
                prog.push(Inst::Jmp(usize::MAX));
                jumps.push(jump);
                let next = prog.len();
                prog[split] = Inst::Split(body, next);
            }
            let end = prog.len();
            for jump in jumps {
                prog[jump] = Inst::Jmp(end);
            }
        }
        Node::Repeat {
            node,
            min,
            max,
            lazy,
        } => {
            for _ in 0..*min {
                compile_node(node, prog, case_insensitive);
            }
            match max {
                None => {
                    let split = prog.len();
                    prog.push(Inst::Split(0, 0));
                    let body = prog.len();
                    compile_node(node, prog, case_insensitive);
                    prog.push(Inst::Jmp(split));
                    let end = prog.len();
                    prog[split] = if *lazy {
                        Inst::Split(end, body)
                    } else {
                        Inst::Split(body, end)
                    };
                }
                Some(max) => {
                    let cap = (*min + 1024).min(*max);
                    let mut splits = Vec::new();
                    for _ in *min..cap {
                        let split = prog.len();
                        prog.push(Inst::Split(0, 0));
                        splits.push(split);
                        compile_node(node, prog, case_insensitive);
                    }
                    let end = prog.len();
                    for split in splits {
                        prog[split] = if *lazy {
                            Inst::Split(end, split + 1)
                        } else {
                            Inst::Split(split + 1, end)
                        };
                    }
                }
            }
        }
    }
}

/// Compile a pattern (UTF-16 units) with the given flags.
pub(crate) fn compile(pattern: &[u16], flags: i32) -> Result<Regex, (String, usize)> {
    let case_insensitive = flags & FLAG_CASE_INSENSITIVE != 0;
    let multiline = flags & FLAG_MULTILINE != 0;
    let dotall = flags & FLAG_DOTALL != 0;
    let literal = flags & FLAG_LITERAL != 0;
    let (node, groups) = if literal {
        let node = if pattern.is_empty() {
            Node::Empty
        } else {
            Node::Concat(pattern.iter().map(|ch| Node::Char(*ch)).collect())
        };
        (node, 0)
    } else {
        let parser = Parser {
            pattern,
            pos: 0,
            groups: 0,
        };
        parser.parse()?
    };
    let mut prog = Vec::new();
    compile_node(&node, &mut prog, case_insensitive);
    prog.push(Inst::Match);
    Ok(Regex {
        prog,
        groups,
        case_insensitive,
        multiline,
        dotall,
    })
}

/// Compile a pattern from a Rust string, reporting errors as `PatternSyntaxException`.
pub(crate) fn compile_from_str(vm: &mut Vm<'_>, pattern: &str) -> Result<Regex, VmError> {
    let units: Vec<u16> = pattern.encode_utf16().collect();
    compile(&units, 0)
        .map_err(|(message, index)| pattern_syntax_error(vm, &message, pattern, index as i32))
}

fn run(
    regex: &Regex,
    mut pc: usize,
    text: &[u16],
    mut pos: usize,
    caps: &mut Vec<Option<(usize, usize)>>,
    steps: &mut usize,
) -> Option<usize> {
    loop {
        *steps += 1;
        if *steps > STEP_LIMIT {
            return None;
        }
        match &regex.prog[pc] {
            Inst::Match => return Some(pos),
            Inst::Char(expected) => {
                if pos < text.len() && eq_char(text[pos], *expected, regex.case_insensitive) {
                    pc += 1;
                    pos += 1;
                    continue;
                }
                return None;
            }
            Inst::Any => {
                if pos < text.len() && (regex.dotall || text[pos] != 0x0A) {
                    pc += 1;
                    pos += 1;
                    continue;
                }
                return None;
            }
            Inst::Class(set) => {
                if pos < text.len() && set.matches(text[pos], regex.case_insensitive) {
                    pc += 1;
                    pos += 1;
                    continue;
                }
                return None;
            }
            Inst::Start => {
                let ok = pos == 0 || (regex.multiline && text[pos - 1] == 0x0A);
                if ok {
                    pc += 1;
                    continue;
                }
                return None;
            }
            Inst::End => {
                let ok = pos == text.len() || (regex.multiline && text[pos] == 0x0A);
                if ok {
                    pc += 1;
                    continue;
                }
                return None;
            }
            Inst::WordBoundary(positive) => {
                let before = pos > 0 && is_word(text[pos - 1]);
                let after = pos < text.len() && is_word(text[pos]);
                if (before != after) == *positive {
                    pc += 1;
                    continue;
                }
                return None;
            }
            Inst::SaveStart(index) => {
                let old = caps[*index];
                caps[*index] = Some((pos, old.map_or(pos, |span| span.1)));
                let result = run(regex, pc + 1, text, pos, caps, steps);
                if result.is_none() {
                    caps[*index] = old;
                }
                return result;
            }
            Inst::SaveEnd(index) => {
                let old = caps[*index];
                caps[*index] = Some((old.map_or(pos, |span| span.0), pos));
                let result = run(regex, pc + 1, text, pos, caps, steps);
                if result.is_none() {
                    caps[*index] = old;
                }
                return result;
            }
            Inst::Jmp(target) => {
                pc = *target;
            }
            Inst::Split(first, second) => {
                let saved = caps.clone();
                if let Some(end) = run(regex, *first, text, pos, caps, steps) {
                    return Some(end);
                }
                *caps = saved;
                pc = *second;
            }
        }
    }
}

/// The leftmost match at or after `from`.
pub(crate) fn find_from(regex: &Regex, text: &[u16], from: usize) -> Option<RegexMatch> {
    for start in from..=text.len() {
        let mut caps = vec![None; regex.groups];
        let mut steps = 0;
        if let Some(end) = run(regex, 0, text, start, &mut caps, &mut steps) {
            return Some(RegexMatch {
                start,
                end,
                groups: caps,
            });
        }
    }
    None
}

/// A match anchored at the start of the text; `require_full` also demands it consumes everything.
pub(crate) fn match_at(regex: &Regex, text: &[u16], require_full: bool) -> Option<RegexMatch> {
    let mut caps = vec![None; regex.groups];
    let mut steps = 0;
    let end = run(regex, 0, text, 0, &mut caps, &mut steps)?;
    if require_full && end != text.len() {
        return None;
    }
    Some(RegexMatch {
        start: 0,
        end,
        groups: caps,
    })
}

/// Expand `$n`/`\c` in a replacement string for one match.
pub(crate) fn expand_replacement(
    matched: &RegexMatch,
    text: &[u16],
    replacement: &[u16],
) -> Vec<u16> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < replacement.len() {
        let ch = replacement[index];
        if ch == b'\\' as u16 && index + 1 < replacement.len() {
            out.push(replacement[index + 1]);
            index += 2;
            continue;
        }
        if ch == b'$' as u16 && index + 1 < replacement.len() {
            let mut digits = String::new();
            let mut cursor = index + 1;
            while cursor < replacement.len()
                && (b'0' as u16..=b'9' as u16).contains(&replacement[cursor])
            {
                digits.push((replacement[cursor] as u8) as char);
                cursor += 1;
            }
            if !digits.is_empty() {
                if let Ok(group) = digits.parse::<usize>() {
                    if group == 0 {
                        out.extend_from_slice(&text[matched.start..matched.end]);
                    } else if let Some(Some((start, end))) = matched.groups.get(group - 1).copied()
                    {
                        out.extend_from_slice(&text[start..end]);
                    }
                    index = cursor;
                    continue;
                }
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

/// Replace every match in `text`.
pub(crate) fn replace_all(regex: &Regex, text: &[u16], replacement: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor <= text.len() {
        let Some(matched) = find_from(regex, text, cursor) else {
            break;
        };
        out.extend_from_slice(&text[cursor..matched.start]);
        out.extend(expand_replacement(&matched, text, replacement));
        cursor = if matched.end > matched.start {
            matched.end
        } else {
            matched.end + 1
        };
    }
    if cursor <= text.len() {
        out.extend_from_slice(&text[cursor.min(text.len())..]);
    }
    out
}

/// Replace the first match in `text`.
pub(crate) fn replace_first(regex: &Regex, text: &[u16], replacement: &[u16]) -> Vec<u16> {
    let Some(matched) = find_from(regex, text, 0) else {
        return text.to_vec();
    };
    let mut out = Vec::new();
    out.extend_from_slice(&text[..matched.start]);
    out.extend(expand_replacement(&matched, text, replacement));
    out.extend_from_slice(&text[matched.end..]);
    out
}

/// Split spans, honoring the JDK limit rules (limit > 0 caps the pieces; limit == 0 drops
/// trailing empty pieces).
pub(crate) fn split_spans(regex: &Regex, text: &[u16], limit: i32) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    let max_pieces = if limit > 0 {
        limit as usize
    } else {
        usize::MAX
    };
    while spans.len() + 1 < max_pieces {
        let Some(matched) = find_from(regex, text, cursor) else {
            break;
        };
        if matched.end == matched.start && matched.start == cursor {
            // Empty match at the cursor: advance without splitting (JDK behavior for zero-width).
            if cursor >= text.len() {
                break;
            }
            cursor += 1;
            continue;
        }
        spans.push((cursor, matched.start));
        cursor = matched.end;
    }
    spans.push((cursor, text.len()));
    if limit == 0 {
        while spans.len() > 1 && spans.last().is_some_and(|(start, end)| start == end) {
            spans.pop();
        }
    }
    spans
}

// -------------------------------------------------------------------------------------------
// java.util.regex natives
// -------------------------------------------------------------------------------------------

const PATTERN_METHODS: [oxjvm_vm::NativeMethodDef; 10] = [
    method(
        "compile",
        "(Ljava/lang/String;)Ljava/util/regex/Pattern;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| pattern_compile(vm, args, 0),
    ),
    method(
        "compile",
        "(Ljava/lang/String;I)Ljava/util/regex/Pattern;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| pattern_compile(vm, args, crate::int_arg(args, 1)),
    ),
    method(
        "matcher",
        "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
        ACC_PUBLIC,
        pattern_matcher,
    ),
    method(
        "pattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let pattern = pattern_field(vm, receiver(args));
            object(pattern)
        },
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| object(pattern_field(vm, receiver(args))),
    ),
    method("flags", "()I", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let flags = crate::lang::read_int_field(vm, this, "flags");
        int(flags)
    }),
    method(
        "quote",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let text = string_arg(vm, args[0])?;
            let mut quoted = String::new();
            for ch in text.chars() {
                if !ch.is_ascii_alphanumeric() && ch != '_' {
                    quoted.push('\\');
                }
                quoted.push(ch);
            }
            object(vm.make_string(&quoted)?)
        },
    ),
    method(
        "matches",
        "(Ljava/lang/String;Ljava/lang/CharSequence;)Z",
        ACC_PUBLIC | ACC_STATIC,
        |vm, _, args| {
            let pattern = string_arg(vm, args[0])?;
            let text = char_sequence_utf16(vm, ref_arg(args, 1))?;
            let regex = compile(&utf16_from_str(&pattern), 0).map_err(|(message, index)| {
                pattern_syntax_error(vm, &message, &pattern, index as i32)
            })?;
            boolean(match_at(&regex, &text, true).is_some())
        },
    ),
    method(
        "split",
        "(Ljava/lang/CharSequence;)[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| pattern_split(vm, args, 0),
    ),
    method(
        "split",
        "(Ljava/lang/CharSequence;I)[Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| pattern_split(vm, args, crate::int_arg(args, 2)),
    ),
];

pub(crate) const PATTERN: oxjvm_vm::NativeClass = class(
    "java/util/regex/Pattern",
    Some("java/lang/Object"),
    &["java/io/Serializable"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("flags", "I", ACC_PRIVATE, None),
        field(
            "UNIX_LINES",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(1)),
        ),
        field(
            "CASE_INSENSITIVE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(FLAG_CASE_INSENSITIVE)),
        ),
        field(
            "MULTILINE",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(FLAG_MULTILINE)),
        ),
        field(
            "LITERAL",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(FLAG_LITERAL)),
        ),
        field(
            "DOTALL",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(FLAG_DOTALL)),
        ),
        field(
            "COMMENTS",
            "I",
            ACC_PUBLIC | ACC_STATIC | ACC_FINAL,
            Some(oxjvm_vm::NativeConstant::Int(4)),
        ),
    ],
    &PATTERN_METHODS,
    None,
);

fn utf16_from_str(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub(crate) fn char_sequence_utf16(
    vm: &mut Vm<'_>,
    reference: ObjectRef,
) -> Result<Vec<u16>, VmError> {
    if reference.is_null() {
        return Err(vm.throw_new("java/lang/NullPointerException", None));
    }
    if let Some(units) = vm.string_utf16(reference) {
        return Ok(units.to_vec());
    }
    let (class, method) =
        vm.resolve_virtual_method(reference, "toString", "()Ljava/lang/String;")?;
    let text = vm.invoke_method(class, method, vec![Value::Ref(reference)])?;
    vm.string_utf16(text.as_ref())
        .map(<[u16]>::to_vec)
        .ok_or_else(|| VmError::internal("CharSequence.toString returned a non-String"))
}

fn pattern_field(vm: &Vm<'_>, this: ObjectRef) -> ObjectRef {
    let class = vm.class_of(this);
    vm.read_ref_field(this, class, "pattern", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL)
}

fn pattern_compile(vm: &mut Vm<'_>, args: &[Value], flags: i32) -> Result<Value, VmError> {
    let pattern = string_arg(vm, args[0])?;
    let units = utf16_from_str(&pattern);
    if let Err((message, index)) = compile(&units, flags) {
        return Err(pattern_syntax_error(vm, &message, &pattern, index as i32));
    }
    let class = vm.resolve_class("java/util/regex/Pattern")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/lang/String;")?;
    let text = vm.make_string(&pattern)?;
    vm.set_instance_ref(instance, declaring, field, text);
    crate::lang::write_int_field(vm, instance, "flags", flags);
    object(instance)
}

fn pattern_syntax_error(vm: &mut Vm<'_>, message: &str, pattern: &str, index: i32) -> VmError {
    let class = vm.resolve_class("java/util/regex/PatternSyntaxException");
    let Ok(class) = class else {
        return vm.throw_new("java/lang/IllegalArgumentException", Some(message));
    };
    let instance = match vm.new_instance(class) {
        Ok(instance) => instance,
        Err(error) => return error,
    };
    let Some((declaring, ctor)) =
        vm.find_method(class, "<init>", "(Ljava/lang/String;Ljava/lang/String;I)V")
    else {
        return vm.throw_new("java/lang/IllegalArgumentException", Some(message));
    };
    let description = match vm.make_string(message) {
        Ok(text) => text,
        Err(error) => return error,
    };
    let regex = match vm.make_string(pattern) {
        Ok(text) => text,
        Err(error) => return error,
    };
    match vm.invoke_method(
        declaring,
        ctor,
        vec![
            Value::Ref(instance),
            Value::Ref(description),
            Value::Ref(regex),
            Value::Int(index),
        ],
    ) {
        Ok(_) => VmError::Thrown(instance),
        Err(error) => error,
    }
}

fn pattern_matcher(
    vm: &mut Vm<'_>,
    _c: oxjvm_vm::NativeContext,
    args: &[Value],
) -> Result<Value, VmError> {
    let this = receiver(args);
    let input = ref_arg(args, 1);
    matcher_new(vm, this, input)
}

fn pattern_split(vm: &mut Vm<'_>, args: &[Value], limit: i32) -> Result<Value, VmError> {
    let this = receiver(args);
    let text = char_sequence_utf16(vm, ref_arg(args, 1))?;
    let (regex, _) = pattern_regex(vm, this)?;
    let spans = split_spans(&regex, &text, limit);
    let mut parts = Vec::with_capacity(spans.len());
    for (start, end) in spans {
        parts.push(String::from_utf16_lossy(&text[start..end]));
    }
    crate::lang::string_array(vm, &parts)
}

/// Recompile a `Pattern` object and return it with its group count.
fn pattern_regex(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(Regex, usize), VmError> {
    let pattern = pattern_field(vm, this);
    if pattern.is_null() {
        return Err(VmError::internal("Pattern without a pattern string"));
    }
    let flags = crate::lang::read_int_field(vm, this, "flags");
    let units = vm
        .string_utf16(pattern)
        .map(<[u16]>::to_vec)
        .ok_or_else(|| VmError::internal("Pattern.pattern is not a String"))?;
    let text = String::from_utf16_lossy(&units);
    let regex = compile(&units, flags)
        .map_err(|(message, index)| pattern_syntax_error(vm, &message, &text, index as i32))?;
    let groups = regex.groups;
    Ok((regex, groups))
}

// -------------------------------------------------------------------------------------------
// Matcher
// -------------------------------------------------------------------------------------------

const MATCHER_METHODS: [oxjvm_vm::NativeMethodDef; 17] = [
    method("matches", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (regex, _) = pattern_regex_for(vm, this)?;
        let text = matcher_text(vm, this)?;
        let matched = match_at(&regex, &text, true);
        matcher_record(vm, this, matched)
    }),
    method("lookingAt", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let (regex, _) = pattern_regex_for(vm, this)?;
        let text = matcher_text(vm, this)?;
        let matched = match_at(&regex, &text, false);
        matcher_record(vm, this, matched)
    }),
    method("find", "()Z", ACC_PUBLIC, |vm, _, args| {
        let this = receiver(args);
        let cursor = crate::lang::read_int_field(vm, this, "cursor");
        matcher_find(vm, this, cursor)
    }),
    method("find", "(I)Z", ACC_PUBLIC, |vm, _, args| {
        let start = crate::int_arg(args, 1);
        if start < 0 {
            return Err(vm.throw_new(
                "java/lang/IndexOutOfBoundsException",
                Some(&alloc::format!("Illegal start index: {start}")),
            ));
        }
        matcher_find(vm, receiver(args), start)
    }),
    method(
        "group",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| matcher_group(vm, receiver(args), 0),
    ),
    method(
        "group",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| matcher_group(vm, receiver(args), crate::int_arg(args, 1)),
    ),
    method("start", "()I", ACC_PUBLIC, |vm, _, args| {
        int(matcher_span_start(vm, receiver(args), 0))
    }),
    method("start", "(I)I", ACC_PUBLIC, |vm, _, args| {
        int(matcher_span_start(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
        ))
    }),
    method("end", "()I", ACC_PUBLIC, |vm, _, args| {
        int(matcher_span_end(vm, receiver(args), 0))
    }),
    method("end", "(I)I", ACC_PUBLIC, |vm, _, args| {
        int(matcher_span_end(
            vm,
            receiver(args),
            crate::int_arg(args, 1),
        ))
    }),
    method("groupCount", "()I", ACC_PUBLIC, |vm, _, args| {
        let (_, groups) = pattern_regex_for(vm, receiver(args))?;
        int(groups.saturating_sub(1) as i32)
    }),
    method(
        "pattern",
        "()Ljava/util/regex/Pattern;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "pattern", "Ljava/util/regex/Pattern;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "reset",
        "()Ljava/util/regex/Matcher;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            crate::lang::write_int_field(vm, this, "cursor", 0);
            matcher_clear(vm, this);
            object(this)
        },
    ),
    method(
        "reset",
        "(Ljava/lang/CharSequence;)Ljava/util/regex/Matcher;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let text = char_sequence_utf16(vm, ref_arg(args, 1))?;
            let class = vm.class_of(this);
            let (declaring, field) = vm.find_field(class, "input", "Ljava/lang/String;")?;
            let text = vm.make_string_utf16(text)?;
            vm.set_instance_ref(this, declaring, field, text);
            crate::lang::write_int_field(vm, this, "cursor", 0);
            matcher_clear(vm, this);
            object(this)
        },
    ),
    method(
        "replaceAll",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| matcher_replace(vm, args, true),
    ),
    method(
        "replaceFirst",
        "(Ljava/lang/String;)Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| matcher_replace(vm, args, false),
    ),
    method(
        "toString",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let (start, end) = (
                crate::lang::read_int_field(vm, this, "matchStart"),
                crate::lang::read_int_field(vm, this, "matchEnd"),
            );
            let text = format!("java.util.regex.Matcher[pattern= start={start} end={end}]");
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const MATCHER: oxjvm_vm::NativeClass = class(
    "java/util/regex/Matcher",
    Some("java/lang/Object"),
    &["java/util/regex/MatchResult"],
    ACC_PUBLIC | ACC_FINAL,
    &[
        field("pattern", "Ljava/util/regex/Pattern;", ACC_PRIVATE, None),
        field("input", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("cursor", "I", ACC_PRIVATE, None),
        field("matchStart", "I", ACC_PRIVATE, None),
        field("matchEnd", "I", ACC_PRIVATE, None),
        field("groups", "[I", ACC_PRIVATE, None),
    ],
    &MATCHER_METHODS,
    None,
);

const MATCH_RESULT_METHODS: [oxjvm_vm::NativeMethodDef; 7] = [
    method(
        "group",
        "()Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "group",
        "(I)Ljava/lang/String;",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "start",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "start",
        "(I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "end",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "end",
        "(I)I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
    method(
        "groupCount",
        "()I",
        ACC_PUBLIC | ACC_ABSTRACT,
        crate::lang::abstract_method_pub,
    ),
];

pub(crate) const MATCH_RESULT: oxjvm_vm::NativeClass = class(
    "java/util/regex/MatchResult",
    Some("java/lang/Object"),
    &[],
    ACC_PUBLIC | ACC_INTERFACE | ACC_ABSTRACT,
    &[],
    &MATCH_RESULT_METHODS,
    None,
);

const PATTERN_SYNTAX_EXCEPTION_METHODS: [oxjvm_vm::NativeMethodDef; 5] = [
    method(
        "<init>",
        "(Ljava/lang/String;Ljava/lang/String;I)V",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            for (index, (name, descriptor)) in [
                ("description", "Ljava/lang/String;"),
                ("pattern", "Ljava/lang/String;"),
            ]
            .iter()
            .enumerate()
            {
                let (declaring, field) = vm.find_field(class, name, descriptor)?;
                vm.set_instance_ref(this, declaring, field, ref_arg(args, index + 1));
            }
            let (declaring, field) = vm.find_field(class, "index", "I")?;
            vm.set_instance_int(this, declaring, field, crate::int_arg(args, 3));
            void()
        },
    ),
    method(
        "getDescription",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "description", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method(
        "getPattern",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            object(
                vm.read_ref_field(this, class, "pattern", "Ljava/lang/String;")
                    .unwrap_or(ObjectRef::NULL),
            )
        },
    ),
    method("getIndex", "()I", ACC_PUBLIC, |vm, _, args| {
        int(crate::lang::read_int_field(vm, receiver(args), "index"))
    }),
    method(
        "getMessage",
        "()Ljava/lang/String;",
        ACC_PUBLIC,
        |vm, _, args| {
            let this = receiver(args);
            let class = vm.class_of(this);
            let description = vm
                .read_string_field(this, class, "description")
                .unwrap_or_default();
            let pattern = vm
                .read_string_field(this, class, "pattern")
                .unwrap_or_default();
            let index = crate::lang::read_int_field(vm, this, "index");
            let text = alloc::format!("{description} near index {index}\n{pattern}");
            object(vm.make_string(&text)?)
        },
    ),
];

pub(crate) const PATTERN_SYNTAX_EXCEPTION: oxjvm_vm::NativeClass = class(
    "java/util/regex/PatternSyntaxException",
    Some("java/lang/IllegalArgumentException"),
    &[],
    ACC_PUBLIC,
    &[
        field("description", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("pattern", "Ljava/lang/String;", ACC_PRIVATE, None),
        field("index", "I", ACC_PRIVATE, None),
    ],
    &PATTERN_SYNTAX_EXCEPTION_METHODS,
    None,
);

fn matcher_new(vm: &mut Vm<'_>, pattern: ObjectRef, input: ObjectRef) -> Result<Value, VmError> {
    let text = char_sequence_utf16(vm, input)?;
    let class = vm.resolve_class("java/util/regex/Matcher")?;
    let instance = vm.new_instance(class)?;
    let (declaring, field) = vm.find_field(class, "pattern", "Ljava/util/regex/Pattern;")?;
    vm.set_instance_ref(instance, declaring, field, pattern);
    let (declaring, field) = vm.find_field(class, "input", "Ljava/lang/String;")?;
    let text = vm.make_string_utf16(text)?;
    vm.set_instance_ref(instance, declaring, field, text);
    matcher_clear(vm, instance);
    object(instance)
}

fn matcher_text(vm: &Vm<'_>, this: ObjectRef) -> Result<Vec<u16>, VmError> {
    let class = vm.class_of(this);
    let input = vm
        .read_ref_field(this, class, "input", "Ljava/lang/String;")
        .unwrap_or(ObjectRef::NULL);
    vm.string_utf16(input)
        .map(<[u16]>::to_vec)
        .ok_or_else(|| VmError::internal("Matcher without input"))
}

fn matcher_clear(vm: &mut Vm<'_>, this: ObjectRef) {
    crate::lang::write_int_field(vm, this, "matchStart", -1);
    crate::lang::write_int_field(vm, this, "matchEnd", -1);
}

fn pattern_regex_for(vm: &mut Vm<'_>, this: ObjectRef) -> Result<(Regex, usize), VmError> {
    let class = vm.class_of(this);
    let pattern = vm
        .read_ref_field(this, class, "pattern", "Ljava/util/regex/Pattern;")
        .unwrap_or(ObjectRef::NULL);
    if pattern.is_null() {
        return Err(VmError::internal("Matcher without a pattern"));
    }
    pattern_regex(vm, pattern)
}

fn matcher_record(
    vm: &mut Vm<'_>,
    this: ObjectRef,
    matched: Option<RegexMatch>,
) -> Result<Value, VmError> {
    match matched {
        Some(matched) => {
            matcher_store(vm, this, &matched)?;
            Ok(Value::Int(1))
        }
        None => {
            matcher_clear(vm, this);
            Ok(Value::Int(0))
        }
    }
}

fn matcher_store(vm: &mut Vm<'_>, this: ObjectRef, matched: &RegexMatch) -> Result<(), VmError> {
    crate::lang::write_int_field(vm, this, "matchStart", matched.start as i32);
    crate::lang::write_int_field(vm, this, "matchEnd", matched.end as i32);
    let class = vm.class_of(this);
    let mut values = alloc::vec![-1i32; (matched.groups.len() + 1) * 2];
    for (index, span) in matched.groups.iter().enumerate() {
        if let Some((start, end)) = span {
            values[index * 2] = *start as i32;
            values[index * 2 + 1] = *end as i32;
        }
    }
    let array = vm.allocate_array_of(oxjvm_vm::ArrayComponent::Int, values.len())?;
    for (index, value) in values.iter().enumerate() {
        vm.array_set(array, index as i32, Value::Int(*value), false)?;
    }
    let (declaring, field) = vm.find_field(class, "groups", "[I")?;
    vm.set_instance_ref(this, declaring, field, array);
    Ok(())
}

fn matcher_find(vm: &mut Vm<'_>, this: ObjectRef, from: i32) -> Result<Value, VmError> {
    let from = from.max(0) as usize;
    let (regex, _) = pattern_regex_for(vm, this)?;
    let text = matcher_text(vm, this)?;
    match find_from(&regex, &text, from) {
        Some(matched) => {
            let next = if matched.end > matched.start {
                matched.end
            } else {
                matched.end + 1
            };
            matcher_store(vm, this, &matched)?;
            crate::lang::write_int_field(vm, this, "cursor", next as i32);
            boolean(true)
        }
        None => {
            matcher_clear(vm, this);
            boolean(false)
        }
    }
}

fn matcher_span(vm: &mut Vm<'_>, this: ObjectRef, group: i32) -> Option<(i32, i32)> {
    if group == 0 {
        let start = crate::lang::read_int_field(vm, this, "matchStart");
        let end = crate::lang::read_int_field(vm, this, "matchEnd");
        if start < 0 || end < 0 {
            return None;
        }
        return Some((start, end));
    }
    let class = vm.class_of(this);
    let array = vm
        .read_ref_field(this, class, "groups", "[I")
        .unwrap_or(ObjectRef::NULL);
    if array.is_null() {
        return None;
    }
    let slot = (group as usize - 1) * 2;
    let start = vm.array_get(array, slot as i32).ok()?.as_int();
    let end = vm.array_get(array, (slot + 1) as i32).ok()?.as_int();
    if start < 0 || end < 0 {
        return None;
    }
    Some((start, end))
}

fn matcher_group(vm: &mut Vm<'_>, this: ObjectRef, group: i32) -> Result<Value, VmError> {
    let Some((start, end)) = matcher_span(vm, this, group) else {
        return object(ObjectRef::NULL);
    };
    let text = matcher_text(vm, this)?;
    let slice: Vec<u16> = text[start as usize..end as usize].to_vec();
    object(vm.make_string_utf16(slice)?)
}

fn matcher_span_start(vm: &mut Vm<'_>, this: ObjectRef, group: i32) -> i32 {
    matcher_span(vm, this, group).map_or(-1, |(start, _)| start)
}

fn matcher_span_end(vm: &mut Vm<'_>, this: ObjectRef, group: i32) -> i32 {
    matcher_span(vm, this, group).map_or(-1, |(_, end)| end)
}

fn matcher_replace(vm: &mut Vm<'_>, args: &[Value], all: bool) -> Result<Value, VmError> {
    let this = receiver(args);
    let replacement = string_arg(vm, args[1])?;
    let (regex, _) = pattern_regex_for(vm, this)?;
    let text = matcher_text(vm, this)?;
    let replacement = utf16_from_str(&replacement);
    let result = if all {
        replace_all(&regex, &text, &replacement)
    } else {
        replace_first(&regex, &text, &replacement)
    };
    object(vm.make_string_utf16(result)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn compiled(pattern: &str) -> Regex {
        compile(&utf16(pattern), 0).expect("pattern")
    }

    fn span(pattern: &str, text: &str) -> Option<(usize, usize)> {
        let regex = compiled(pattern);
        span_of(&regex, text)
    }

    fn span_of(regex: &Regex, text: &str) -> Option<(usize, usize)> {
        let text = utf16(text);
        find_from(regex, &text, 0).map(|matched| (matched.start, matched.end))
    }

    #[test]
    fn literals_and_anchors() {
        assert_eq!(span("abc", "xabcy"), Some((1, 4)));
        assert_eq!(span("^abc$", "abc"), Some((0, 3)));
        assert_eq!(span("^abc$", "xabc"), None);
        assert_eq!(span("abc", "ab"), None);
    }

    #[test]
    fn dot_and_classes() {
        assert_eq!(span("a.c", "azc"), Some((0, 3)));
        assert_eq!(span("a.c", "a\nc"), None);
        assert_eq!(span("[a-c]+", "xxabcxx"), Some((2, 5)));
        assert_eq!(span("[^a-z]", "abcD"), Some((3, 4)));
        assert_eq!(span("\\d+", "ab123"), Some((2, 5)));
        assert_eq!(span("\\w+", "  hi_9 "), Some((2, 6)));
        assert_eq!(span("\\s", "ab c"), Some((2, 3)));
    }

    #[test]
    fn quantifiers() {
        assert_eq!(span("a*", "aaa"), Some((0, 3)));
        assert_eq!(span("ab?c", "ac"), Some((0, 2)));
        assert_eq!(span("a{2,3}", "aaaa"), Some((0, 3)));
        assert_eq!(span("a{2,}", "aaaa"), Some((0, 4)));
        assert_eq!(span("a+?", "aaa"), Some((0, 1)));
        assert_eq!(span("<.*?>", "<a><b>"), Some((0, 3)));
    }

    #[test]
    fn groups_and_alternation() {
        let regex = compiled("(ab|cd)+");
        let text = utf16("xabcdy");
        let matched = find_from(&regex, &text, 0).expect("match");
        assert_eq!((matched.start, matched.end), (1, 5));
        assert_eq!(matched.groups[0], Some((3, 5)));
    }

    #[test]
    fn word_boundaries() {
        assert_eq!(span("\\bcat\\b", "a cat!"), Some((2, 5)));
        assert_eq!(span("\\bcat\\b", "scatter"), None);
    }

    #[test]
    fn full_match_and_looking_at() {
        let regex = compiled("a+");
        assert!(match_at(&regex, &utf16("aaa"), true).is_some());
        assert!(match_at(&regex, &utf16("aaab"), true).is_none());
        assert!(match_at(&regex, &utf16("aaab"), false).is_some());
    }

    #[test]
    fn splits() {
        let regex = compiled(",");
        let text = utf16("a,,b,");
        let parts: Vec<String> = split_spans(&regex, &text, 0)
            .iter()
            .map(|(start, end)| String::from_utf16_lossy(&text[*start..*end]))
            .collect();
        assert_eq!(parts.join("|"), "a||b");
        let parts: Vec<String> = split_spans(&regex, &text, -1)
            .iter()
            .map(|(start, end)| String::from_utf16_lossy(&text[*start..*end]))
            .collect();
        assert_eq!(parts.join("|"), "a||b|");
    }

    #[test]
    fn replacements() {
        let regex = compiled("(\\w+)@(\\w+)");
        let text = utf16("mail me@here now");
        let out = replace_all(&regex, &text, &utf16("$2/$1"));
        assert_eq!(String::from_utf16_lossy(&out), "mail here/me now");
        let out = replace_first(&regex, &text, &utf16("[$0]"));
        assert_eq!(String::from_utf16_lossy(&out), "mail [me@here] now");
    }

    #[test]
    fn rejects_backreferences() {
        assert!(compile(&utf16("(a)\\1"), 0).is_err());
    }

    #[test]
    fn literal_flag() {
        let regex = compile(&utf16("a.c"), FLAG_LITERAL).expect("literal");
        assert_eq!(span_of(&regex, "a.c"), Some((0, 3)));
        assert_eq!(span_of(&regex, "abc"), None);
    }
}
