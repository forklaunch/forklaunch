//! Parser from regex patterns to IR

use crate::compile_ledger as budget;
use crate::{
    api, charclasses,
    codepointset::{CodePointSet, Interval, interval_contains},
    ir,
    types::{
        BracketContents, CaptureGroupID, CaptureGroupName, CharacterClassType, MAX_CAPTURE_GROUPS,
        MAX_LOOPS,
    },
    unicode::{
        self, PropertyEscapeKind, unicode_property_from_str, unicode_property_name_from_str,
    },
    unicodetables::{id_continue_ranges, id_start_ranges},
    util::to_char_sat,
};
use core::{fmt, iter::Peekable};
#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(not(feature = "std"))]
use {
    alloc::{boxed::Box, string::String, vec::Vec},
    hashbrown::HashMap,
};

trait KeyCost {
    fn key_cost(&self) -> usize;
}
impl KeyCost for usize {
    fn key_cost(&self) -> usize {
        1
    }
}
impl KeyCost for String {
    fn key_cost(&self) -> usize {
        self.len().saturating_add(1)
    }
}
enum ParserMap<K, V> {
    Unlimited(HashMap<K, V>),
    Limited(Vec<(K, V)>),
}
impl<K: Eq + core::hash::Hash + KeyCost, V> ParserMap<K, V> {
    fn new() -> Self {
        if budget::active() {
            Self::Limited(Vec::new())
        } else {
            Self::Unlimited(HashMap::new())
        }
    }
    fn is_empty(&self) -> bool {
        match self {
            Self::Unlimited(m) => m.is_empty(),
            Self::Limited(m) => m.is_empty(),
        }
    }
    fn get(&self, key: &K) -> Option<&V> {
        match self {
            Self::Unlimited(m) => m.get(key),
            Self::Limited(m) => {
                for (k, v) in m {
                    if !budget::charge(0, k.key_cost().saturating_add(key.key_cost())) {
                        return None;
                    }
                    if k == key {
                        return Some(v);
                    }
                }
                None
            }
        }
    }
    fn get_or_insert(&mut self, key: K, value: V) -> Option<&mut V> {
        match self {
            Self::Unlimited(m) => Some(m.entry(key).or_insert(value)),
            Self::Limited(m) => {
                let mut found = None;
                for (i, (k, _)) in m.iter().enumerate() {
                    if !budget::charge(0, k.key_cost().saturating_add(key.key_cost())) {
                        return None;
                    }
                    if k == &key {
                        found = Some(i);
                        break;
                    }
                }
                let index = if let Some(i) = found {
                    i
                } else {
                    if !budget::reserve(m, 1) {
                        return None;
                    }
                    m.push((key, value));
                    m.len() - 1
                };
                Some(&mut m[index].1)
            }
        }
    }
    fn insert(&mut self, key: K, value: V) -> bool {
        match self {
            Self::Unlimited(m) => {
                m.insert(key, value);
                true
            }
            Self::Limited(m) => {
                for (k, v) in m.iter_mut() {
                    if !budget::charge(0, k.key_cost().saturating_add(key.key_cost())) {
                        return false;
                    }
                    if k == &key {
                        *v = value;
                        return true;
                    }
                }
                budget::push(m, (key, value))
            }
        }
    }
    fn remove(&mut self, key: &K) {
        match self {
            Self::Unlimited(m) => {
                m.remove(key);
            }
            Self::Limited(m) => {
                if !budget::charge(0, m.len().saturating_mul(key.key_cost())) {
                    return;
                }
                if let Some(i) = m.iter().position(|(k, _)| k == key) {
                    m.remove(i);
                }
            }
        }
    }
    fn values(&self) -> Box<dyn Iterator<Item = &V> + '_> {
        match self {
            Self::Unlimited(m) => Box::new(m.values()),
            Self::Limited(m) => Box::new(m.iter().map(|(_, v)| v)),
        }
    }
}

/// Represents an error encountered during regex compilation.
///
/// The text contains a human-readable error message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Error {
    pub text: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

enum ClassAtom {
    CodePoint(u32),
    CharacterClass {
        class_type: CharacterClassType,
        positive: bool,
    },
    Range {
        iv: CodePointSet,
        negate: bool,
    },
}

/// Represents the result of a class set.
#[derive(Debug, Clone)]
struct ClassSet {
    codepoints: CodePointSet,
    alternatives: ClassSetAlternativeStrings,
}

impl ClassSet {
    fn new() -> Self {
        ClassSet {
            codepoints: CodePointSet::new(),
            alternatives: ClassSetAlternativeStrings::new(),
        }
    }

    fn node(mut self, icase: bool, negate_set: bool) -> ir::Node {
        if self.alternatives.0.is_empty() {
            return ir::Node::Bracket(BracketContents {
                invert: negate_set,
                cps: self.codepoints,
            });
        }
        if !budget::charge(
            self.alternatives
                .0
                .len()
                .saturating_mul(core::mem::size_of::<Vec<u32>>()),
            self.alternatives
                .0
                .len()
                .saturating_mul(self.alternatives.0.len()),
        ) {
            return ir::Node::Empty;
        }
        self.alternatives
            .0
            .sort_by_key(|text| core::cmp::Reverse(text.len()));
        let has_empty = self.alternatives.0.iter().any(Vec::is_empty);
        self.alternatives.0.retain(|text| !text.is_empty());
        let mut nodes = self.alternatives.to_nodes(icase);
        if !budget::reserve(&mut nodes, 2) {
            return ir::Node::Empty;
        }
        if !self.codepoints.is_empty() {
            nodes.push(ir::Node::Bracket(BracketContents {
                invert: negate_set,
                cps: self.codepoints,
            }));
        }
        if has_empty {
            nodes.push(ir::Node::Empty);
        }
        make_alt(nodes)
    }

    fn union_operand(&mut self, operand: ClassSetOperand) {
        match operand {
            ClassSetOperand::ClassSetCharacter(c) => {
                self.codepoints.add_one(c);
            }
            ClassSetOperand::CharacterClassEscape(cps) => {
                self.codepoints.add_set(cps);
            }
            ClassSetOperand::Class(class) => {
                self.codepoints.add_set(class.codepoints);
                self.alternatives.extend(class.alternatives);
            }
            ClassSetOperand::ClassStringDisjunction(s) => {
                self.alternatives.extend(s);
            }
        }
    }

    fn charge_operand_scan(&self, operand: &ClassSetOperand) -> bool {
        let own = self
            .codepoints
            .intervals()
            .len()
            .saturating_add(self.alternatives.0.len())
            .saturating_add(1);
        let other = match operand {
            ClassSetOperand::ClassSetCharacter(_) => 1,
            ClassSetOperand::CharacterClassEscape(cps) => cps.intervals().len().saturating_add(1),
            ClassSetOperand::Class(class) => class
                .codepoints
                .intervals()
                .len()
                .saturating_add(class.alternatives.0.len())
                .saturating_add(1),
            ClassSetOperand::ClassStringDisjunction(strings) => strings.0.len().saturating_add(1),
        };
        // At most four operand scans, with codepoint membership bounded by
        // interval count. String-to-string comparisons debit separately.
        budget::charge(0, own.saturating_mul(other).saturating_mul(4))
    }
    fn intersect_operand(&mut self, operand: ClassSetOperand) {
        if !self.charge_operand_scan(&operand) {
            return;
        }
        if !budget::charge(
            core::mem::size_of::<Interval>()
                + core::mem::size_of::<Vec<u32>>()
                + core::mem::size_of::<u32>(),
            self.alternatives.0.len(),
        ) {
            return;
        }
        match operand {
            ClassSetOperand::ClassSetCharacter(c) => {
                if self.codepoints.contains(c) {
                    self.codepoints =
                        CodePointSet::from_sorted_disjoint_intervals(Vec::from([Interval {
                            first: c,
                            last: c,
                        }]));
                } else {
                    self.codepoints.clear();
                }
                let mut found_alternative = false;
                for alternative in &mut self.alternatives.0 {
                    if alternative.len() == 1 && alternative[0] == c {
                        found_alternative = true;
                        break;
                    }
                }
                self.alternatives.0.clear();
                if found_alternative {
                    if !budget::reserve(&mut self.alternatives.0, 1) {
                        return;
                    }
                    self.alternatives.0.push(Vec::from([c]));
                }
            }
            ClassSetOperand::CharacterClassEscape(cps) => {
                self.codepoints.intersect(cps.intervals());
                let mut retained = Vec::new();
                for alternative in &self.alternatives.0 {
                    if alternative.len() == 1 && cps.contains(alternative[0]) {
                        if !budget::reserve(&mut retained, 1) {
                            return;
                        }
                        retained.push(budget::copy(alternative));
                    }
                }
                self.alternatives.0 = retained;
            }
            ClassSetOperand::Class(class) => {
                let mut retained_codepoints = CodePointSet::new();
                for alternative in &class.alternatives.0 {
                    if alternative.len() == 1 && self.codepoints.contains(alternative[0]) {
                        retained_codepoints.add_one(alternative[0]);
                    }
                }
                let mut retained_alternatives = ClassSetAlternativeStrings::new();
                for alternative in &self.alternatives.0 {
                    if alternative.len() == 1 && class.codepoints.contains(alternative[0]) {
                        if !budget::reserve(&mut retained_alternatives.0, 1) {
                            return;
                        }
                        retained_alternatives.0.push(budget::copy(alternative));
                    }
                }
                self.codepoints.intersect(class.codepoints.intervals());
                self.codepoints.add_set(retained_codepoints);
                self.alternatives.intersect(class.alternatives);
                self.alternatives.extend(retained_alternatives);
            }
            ClassSetOperand::ClassStringDisjunction(s) => {
                let mut retained = CodePointSet::new();
                for alternative in &s.0 {
                    if alternative.len() == 1 && self.codepoints.contains(alternative[0]) {
                        retained.add_one(alternative[0]);
                    }
                }
                self.codepoints = retained;
                self.alternatives.intersect(s);
            }
        }
    }

    fn subtract_operand(&mut self, operand: ClassSetOperand) {
        if !self.charge_operand_scan(&operand) {
            return;
        }
        if !budget::charge(
            core::mem::size_of::<Vec<u32>>() + core::mem::size_of::<u32>(),
            self.alternatives.0.len(),
        ) {
            return;
        }
        match operand {
            ClassSetOperand::ClassSetCharacter(c) => {
                self.codepoints.remove(&[Interval { first: c, last: c }]);
                self.alternatives
                    .remove(ClassSetAlternativeStrings(Vec::from([Vec::from([c])])));
            }
            ClassSetOperand::CharacterClassEscape(cps) => {
                self.codepoints.remove(cps.intervals());
                let mut to_remove = ClassSetAlternativeStrings::new();
                for alternative in &self.alternatives.0 {
                    if alternative.len() == 1 && cps.contains(alternative[0]) {
                        if !budget::reserve(&mut to_remove.0, 1) {
                            return;
                        }
                        to_remove.0.push(budget::copy(alternative));
                    }
                }
                self.alternatives.remove(to_remove);
            }
            ClassSetOperand::Class(class) => {
                let mut codepoints_removed = CodePointSet::new();
                for alternative in &class.alternatives.0 {
                    if alternative.len() == 1 && self.codepoints.contains(alternative[0]) {
                        codepoints_removed.add_one(alternative[0]);
                    }
                }
                let mut alternatives_removed = ClassSetAlternativeStrings::new();
                for alternative in &self.alternatives.0 {
                    if alternative.len() == 1 && class.codepoints.contains(alternative[0]) {
                        if !budget::reserve(&mut alternatives_removed.0, 1) {
                            return;
                        }
                        alternatives_removed.0.push(budget::copy(alternative));
                    }
                }
                self.codepoints.remove(codepoints_removed.intervals());
                self.codepoints.remove(class.codepoints.intervals());
                self.alternatives.remove(alternatives_removed);
                self.alternatives.remove(class.alternatives);
            }
            ClassSetOperand::ClassStringDisjunction(s) => {
                let mut to_remove = CodePointSet::new();
                for alternative in &s.0 {
                    if alternative.len() == 1 && self.codepoints.contains(alternative[0]) {
                        to_remove.add_one(alternative[0]);
                    }
                }
                self.codepoints.remove(to_remove.intervals());
                self.alternatives.remove(s);
            }
        }
    }
}

/// Represents all different types of class set operands.
#[derive(Debug, Clone)]
enum ClassSetOperand {
    ClassSetCharacter(u32),
    CharacterClassEscape(CodePointSet),
    Class(ClassSet),
    ClassStringDisjunction(ClassSetAlternativeStrings),
}

#[derive(Debug)]
struct ClassSetAlternativeStrings(Vec<Vec<u32>>);

impl Clone for ClassSetAlternativeStrings {
    fn clone(&self) -> Self {
        let mut values = Vec::new();
        if budget::reserve(&mut values, self.0.len()) {
            for s in &self.0 {
                values.push(budget::copy(s));
                if budget::failed() {
                    break;
                }
            }
        }
        Self(values)
    }
}
impl ClassSetAlternativeStrings {
    fn new() -> Self {
        ClassSetAlternativeStrings(Vec::new())
    }

    fn extend(&mut self, other: Self) {
        if budget::reserve(&mut self.0, other.0.len()) {
            self.0.extend(other.0);
        }
    }

    fn intersect(&mut self, other: Self) {
        let mut retained = Vec::new();
        if !self.charge_comparison(&other) {
            return;
        }
        for string in &self.0 {
            for other_string in &other.0 {
                if string == other_string {
                    if !budget::reserve(&mut retained, 1) {
                        return;
                    }
                    retained.push(budget::copy(string));
                    break;
                }
            }
        }
        self.0 = retained;
    }

    fn remove(&mut self, other: Self) {
        if !self.charge_comparison(&other) {
            return;
        }
        self.0.retain(|string| !other.0.contains(string));
    }

    fn charge_comparison(&self, other: &Self) -> bool {
        if !budget::charge(0, self.0.len().saturating_add(other.0.len())) {
            return false;
        }
        let max_len = self
            .0
            .iter()
            .chain(&other.0)
            .map(Vec::len)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        budget::charge(
            0,
            self.0
                .len()
                .saturating_mul(other.0.len())
                .saturating_mul(max_len),
        )
    }
    fn to_nodes(&self, icase: bool) -> Vec<ir::Node> {
        let mut nodes = Vec::new();
        if !budget::reserve(&mut nodes, self.0.len()) {
            return nodes;
        }
        for string in &self.0 {
            if !budget::charge(
                string
                    .len()
                    .saturating_mul(core::mem::size_of::<ir::Node>()),
                string.len(),
            ) {
                return Vec::new();
            }
            nodes.push(ir::Node::Cat(
                string
                    .iter()
                    .map(|cp| ir::Node::Char {
                        c: if icase {
                            unicode::fold_code_point(*cp, true)
                        } else {
                            *cp
                        },
                        icase,
                    })
                    .collect::<Vec<_>>(),
            ));
        }
        nodes
    }
}

fn error<S, T>(text: S) -> Result<T, Error>
where
    S: AsRef<str>,
{
    let text = text.as_ref();
    if !budget::charge(text.len(), text.len()) {
        return Err(Error {
            text: String::new(),
        });
    }
    Err(Error {
        text: text.to_owned(),
    })
}

fn error_join<T>(prefix: &str, suffix: &str) -> Result<T, Error> {
    let Some(len) = prefix.len().checked_add(suffix.len()) else {
        budget::arithmetic_failure();
        return error("");
    };
    if !budget::charge(len, len) {
        return error("");
    }
    let mut text = String::new();
    if text.try_reserve_exact(len).is_err() {
        budget::arithmetic_failure();
        return error("");
    }
    text.push_str(prefix);
    text.push_str(suffix);
    Err(Error { text })
}

fn make_cat(nodes: ir::NodeList) -> ir::Node {
    match nodes.len() {
        0 => ir::Node::Empty,
        1 => nodes.into_iter().next().unwrap(),
        _ => ir::Node::Cat(nodes),
    }
}

fn make_alt(nodes: ir::NodeList) -> ir::Node {
    // Balanced trees preserve left-to-right alternatives while preventing
    // Unicode string properties from creating thousands of recursive drops.
    fn build(nodes: &mut impl Iterator<Item = ir::Node>, len: usize) -> ir::Node {
        match len {
            0 => ir::Node::Empty,
            1 => nodes.next().unwrap(),
            _ => {
                let left = len / 2;
                ir::Node::Alt(
                    Box::new(build(nodes, left)),
                    Box::new(build(nodes, len - left)),
                )
            }
        }
    }
    let len = nodes.len();
    if !budget::charge(
        len.saturating_mul(2)
            .saturating_mul(core::mem::size_of::<ir::Node>()),
        len,
    ) {
        return ir::Node::Empty;
    }
    build(&mut nodes.into_iter(), len)
}

/// \return a CodePointSet for a given character escape (positive or negative).
/// See ES9 21.2.2.12.
/// Returns the positive (non-inverted) code point set for a character class.
fn codepoints_from_class_positive(ct: CharacterClassType) -> CodePointSet {
    let mut cps;
    match ct {
        CharacterClassType::Digits => {
            cps = CodePointSet::from_sorted_disjoint_intervals(budget::copy(&charclasses::DIGITS))
        }
        CharacterClassType::Words => {
            cps =
                CodePointSet::from_sorted_disjoint_intervals(budget::copy(&charclasses::WORD_CHARS))
        }
        CharacterClassType::Spaces => {
            cps = CodePointSet::from_sorted_disjoint_intervals(budget::copy(
                &charclasses::WHITESPACE,
            ));
            for &iv in charclasses::LINE_TERMINATOR.iter() {
                cps.add(iv)
            }
        }
    }
    cps
}

/// Returns code points for a character class, optionally inverted.
fn codepoints_from_class(ct: CharacterClassType, positive: bool) -> CodePointSet {
    let cps = codepoints_from_class_positive(ct);
    if positive { cps } else { cps.inverted() }
}

/// \return a Bracket for a given character escape (positive or negative).
/// For icase mode, we expand the positive set first, then invert if needed.
fn make_bracket_class(
    ct: CharacterClassType,
    positive: bool,
    icase: bool,
    unicode: bool,
) -> ir::Node {
    // Get the positive (non-inverted) set, perform any icase expansion, then maybe invert.
    let mut cps = codepoints_from_class_positive(ct);
    if icase {
        cps = if unicode {
            unicode::add_icase_code_points(cps)
        } else {
            unicode::add_legacy_icase_code_points(cps)
        };
    }
    if !positive {
        cps = cps.inverted();
    }
    ir::Node::Bracket(BracketContents { invert: false, cps })
}

fn add_class_atom(bc: &mut BracketContents, atom: ClassAtom) {
    match atom {
        ClassAtom::CodePoint(c) => bc.cps.add_one(c),
        ClassAtom::CharacterClass {
            class_type,
            positive,
        } => {
            bc.cps.add_set(codepoints_from_class(class_type, positive));
        }
        ClassAtom::Range { iv, negate } => {
            if negate {
                bc.cps.add_set(iv.inverted());
            } else {
                bc.cps.add_set(iv);
            }
        }
    }
}

struct LookaroundParams {
    negate: bool,
    backwards: bool,
}

/// Represents an alternative path in a regex pattern.
/// For example, in `/(?<a>x)|(?<a>y)/`, the two occurrences of 'a' are in different
/// alternative paths (separated by |), so they don't conflict.
/// Each element in the vector is (depth, alternative_index) where:
/// - depth: parenthesis nesting level (0 = top level)
/// - alternative_index: which alternative at that depth (0 = first, 1 = second after |, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
struct AlternativePath {
    /// Vector of (depth, alternative_index) pairs representing the path through alternatives
    segments: Vec<(usize, usize)>,
}

impl AlternativePath {
    /// Check if two alternative paths conflict (i.e., are in the same alternative branch).
    /// Two paths conflict if they share the same alternative indices at all common depth levels.
    /// Example:
    ///   - [(0, 0)] and [(0, 0), (1, 0)] conflict (second is nested within first)
    ///   - [(0, 0)] and [(0, 1)] don't conflict (different alternatives at depth 0)
    fn conflicts_with(&self, other: &AlternativePath) -> bool {
        let min_len = self.segments.len().min(other.segments.len());
        self.segments[..min_len] == other.segments[..min_len]
    }
}

/// Represents the state used to parse a regex.
struct Parser<I>
where
    I: Iterator<Item = u32>,
{
    /// The remaining input.
    input: Peekable<I>,

    /// Flags used.
    flags: api::Flags,

    /// Number of loops.
    loop_count: u32,

    /// Number of capturing groups.
    group_count: CaptureGroupID,

    /// Maximum number of capturing groups.
    group_count_max: u32,

    /// A map each from capture group name to corresponding group indices in order.
    /// Note duplicate names may appear in distinct alternations, per the TC39 proposal.
    /// See <https://github.com/tc39/proposal-duplicate-named-capturing-groups>
    named_group_indices: ParserMap<CaptureGroupName, Vec<u32>>,

    /// Whether a lookbehind was encountered.
    has_lookbehind: bool,
}

impl<I> Parser<I>
where
    I: Iterator<Item = u32> + Clone,
{
    /// Consume a character, returning it.
    fn consume<C: Into<u32>>(&mut self, c: C) -> u32 {
        let nc = self.input.next();
        core::debug_assert!(nc == Some(c.into()), "char was not next");
        nc.unwrap()
    }

    /// If our contents begin with the char c, consume it from our contents
    /// and return true. Otherwise return false.
    fn try_consume<C: Into<u32>>(&mut self, c: C) -> bool {
        self.input.next_if_eq(&c.into()).is_some()
    }

    /// If our contents begin with the string \p s, consume it from our contents
    /// and return true. Otherwise return false.
    fn try_consume_str(&mut self, s: &str) -> bool {
        if !budget::charge(0, s.len()) {
            return false;
        }
        let mut cursor = self.input.clone();
        for c1 in s.chars() {
            if cursor.next() != Some(c1 as u32) {
                return false;
            }
        }
        self.input = cursor;
        true
    }

    /// Fold a character if icase.
    fn fold_if_icase(&self, c: u32) -> u32 {
        if self.flags.icase {
            unicode::fold_code_point(c, self.flags.unicode)
        } else {
            c
        }
    }

    /// Peek at the next character.
    fn peek(&mut self) -> Option<u32> {
        if !budget::charge(0, 1) {
            return None;
        }
        self.input.peek().copied()
    }

    /// \return the next character.
    fn next(&mut self) -> Option<u32> {
        if !budget::charge(0, 1) {
            return None;
        }
        self.input.next()
    }

    fn try_parse(&mut self) -> Result<ir::Regex, Error> {
        if !budget::charge(2 * core::mem::size_of::<ir::Node>(), 1) {
            return error("");
        }
        self.parse_capture_groups()?;

        // Parse a catenation. If we consume everything, it's success. If there's
        // something left, it's an error (for example, an excess closing paren).
        let body = self.consume_disjunction()?;
        match self.input.peek().copied() {
            Some(c) if c == ')' as u32 => error("Unbalanced parenthesis"),
            Some(c) => {
                if !budget::charge(64, 1) {
                    return error("");
                }
                error(format!(
                    "Unexpected char: {}",
                    char::from_u32(c)
                        .map(String::from)
                        .unwrap_or_else(|| format!("\\u{c:04X}"))
                ))
            }
            None => self.finalize(ir::Regex {
                node: make_cat(vec![body, ir::Node::Goal]),
                flags: self.flags,
            }),
        }
    }

    /// ES6 21.2.2.3 Disjunction.
    fn consume_disjunction(&mut self) -> Result<ir::Node, Error> {
        let mut terms = Vec::new();
        if !budget::reserve(&mut terms, 1) {
            return error("");
        }
        terms.push(self.consume_term()?);
        while self.try_consume('|') {
            if !budget::reserve(&mut terms, 1) {
                return error("");
            }
            terms.push(self.consume_term()?)
        }
        Ok(make_alt(terms))
    }

    /// ES6 21.2.2.5 Term.
    fn consume_term(&mut self) -> Result<ir::Node, Error> {
        let mut result: Vec<ir::Node> = Vec::new();
        loop {
            if !budget::reserve(&mut result, 3)
                || !budget::charge(core::mem::size_of::<ir::Node>().saturating_mul(3), 1)
            {
                return error("");
            }
            let start_group = self.group_count;
            let mut start_offset = result.len();
            let mut quantifier_allowed = true;

            let nc = self.peek();
            if nc.is_none() {
                return Ok(make_cat(result));
            }
            let c = nc.unwrap();
            match to_char_sat(c) {
                // A concatenation is terminated by closing parens or vertical bar (alternations).
                ')' | '|' => break,
                // Term :: Assertion :: ^
                '^' => {
                    self.consume('^');
                    result.push(ir::Node::Anchor {
                        anchor_type: ir::AnchorType::StartOfLine,
                        multiline: self.flags.multiline,
                    });
                    quantifier_allowed = false;
                }
                // Term :: Assertion :: $
                '$' => {
                    self.consume('$');
                    result.push(ir::Node::Anchor {
                        anchor_type: ir::AnchorType::EndOfLine,
                        multiline: self.flags.multiline,
                    });
                    quantifier_allowed = false;
                }

                '\\' => {
                    self.consume('\\');
                    let Some(c) = self.peek() else {
                        return error("Incomplete escape");
                    };
                    match to_char_sat(c) {
                        // Term :: Assertion :: \b
                        'b' => {
                            self.consume('b');
                            result.push(ir::Node::WordBoundary { invert: false });
                        }
                        // Term :: Assertion :: \B
                        'B' => {
                            self.consume('B');
                            result.push(ir::Node::WordBoundary { invert: true });
                        }
                        // Term :: Atom :: \ AtomEscape :: CharacterEscape :: c AsciiLetter
                        // Term :: ExtendedAtom :: \ [lookahead = c]
                        'c' if !self.flags.unicode => {
                            self.consume('c');
                            if self
                                .peek()
                                .and_then(char::from_u32)
                                .map(|c| c.is_ascii_alphabetic())
                                == Some(true)
                            {
                                result.push(ir::Node::Char {
                                    c: self.next().expect("char was not next") % 32,
                                    icase: self.flags.icase,
                                });
                            } else {
                                start_offset += 1;
                                result.push(ir::Node::Char {
                                    c: u32::from('\\'),
                                    icase: self.flags.icase,
                                });
                                result.push(ir::Node::Char {
                                    c: u32::from('c'),
                                    icase: self.flags.icase,
                                });
                            }
                        }
                        // Term :: Atom :: \ AtomEscape
                        _ => {
                            result.push(self.consume_atom_escape()?);
                        }
                    }
                }

                // Term :: Atom :: .
                '.' => {
                    self.consume('.');
                    result.push(if self.flags.dot_all {
                        ir::Node::MatchAny
                    } else {
                        ir::Node::MatchAnyExceptLineTerminator
                    });
                }

                '(' => {
                    if self.try_consume_str("(?=") {
                        // Positive lookahead.
                        quantifier_allowed = !self.flags.unicode;
                        result.push(self.consume_lookaround_assertion(LookaroundParams {
                            negate: false,
                            backwards: false,
                        })?);
                    } else if self.try_consume_str("(?!") {
                        // Negative lookahead.
                        quantifier_allowed = !self.flags.unicode;
                        result.push(self.consume_lookaround_assertion(LookaroundParams {
                            negate: true,
                            backwards: false,
                        })?);
                    } else if self.try_consume_str("(?<=") {
                        // Positive lookbehind.
                        quantifier_allowed = false;
                        self.has_lookbehind = true;
                        result.push(self.consume_lookaround_assertion(LookaroundParams {
                            negate: false,
                            backwards: true,
                        })?);
                    } else if self.try_consume_str("(?<!") {
                        // Negative lookbehind.
                        quantifier_allowed = false;
                        self.has_lookbehind = true;
                        result.push(self.consume_lookaround_assertion(LookaroundParams {
                            negate: true,
                            backwards: true,
                        })?);
                    } else if self.try_consume_str("(?:") {
                        // Non-capturing group.
                        result.push(self.consume_disjunction()?);
                    } else if let Some(group) = self.try_consume_modifier_group()? {
                        result.push(group);
                    } else {
                        // Capturing group.
                        self.consume('(');
                        let group = self.group_count;
                        if self.group_count as usize >= MAX_CAPTURE_GROUPS {
                            return error("Capture group count limit exceeded");
                        }
                        self.group_count += 1;

                        // Maybe the capture group has a name!
                        let mut group_name = None;
                        if self.try_consume_str("?") {
                            group_name = self.try_consume_named_capture_group_name();
                            if group_name.is_none() {
                                return error("Invalid token at named capture group identifier");
                            };
                        }
                        let contents = Box::new(self.consume_disjunction()?);
                        result.push(ir::Node::CaptureGroup {
                            id: group,
                            contents,
                            name: group_name,
                        })
                    }
                    if !self.try_consume(')') {
                        return error("Unbalanced parenthesis");
                    }
                }

                // CharacterClass :: ClassContents :: ClassSetExpression
                '[' if self.flags.unicode_sets => {
                    self.consume('[');
                    let negate_set = self.try_consume('^');
                    result.push(
                        self.consume_class_set_expression(negate_set)?
                            .node(self.flags.icase, negate_set),
                    );
                }

                '[' => {
                    result.push(self.consume_bracket()?);
                }

                // Term :: ExtendedAtom :: InvalidBracedQuantifier
                '{' if !self.flags.unicode => {
                    if self.try_consume_braced_quantifier().is_some() {
                        return error("Invalid braced quantifier");
                    }

                    // Term :: ExtendedAtom :: ExtendedPatternCharacter
                    result.push(ir::Node::Char {
                        c: self.consume(c),
                        icase: self.flags.icase,
                    })
                }

                // Term :: Atom :: PatternCharacter :: SourceCharacter but not ^ $ \ . * + ? ( ) [ ] { } |
                '*' | '+' | '?' | ']' | '{' | '}' if self.flags.unicode => {
                    return error("Invalid atom character");
                }

                // Term :: ExtendedAtom :: SourceCharacter but not ^ $ \ . * + ? ( ) [ |
                '*' | '+' | '?' => {
                    return error("Invalid atom character");
                }

                // Term :: Atom :: PatternCharacter
                // Term :: ExtendedAtom :: ExtendedPatternCharacter
                _ => {
                    self.consume(c);
                    result.push(ir::Node::Char {
                        c: self.fold_if_icase(c),
                        icase: self.flags.icase,
                    })
                }
            }

            // We just parsed a term; try parsing a quantifier.
            if let Some(quant) = self.try_consume_quantifier()? {
                if !quantifier_allowed {
                    return error("Quantifier not allowed here");
                }
                // Validate the quantifier.
                // Note we don't want to do this as part of parsing the quantiifer in some cases
                // an incomplete quantifier is not recognized as a quantifier, e.g. `/{3/` is
                // valid.
                if matches!(quant.max, Some(max) if quant.min > max) {
                    return error("Invalid quantifier");
                }
                if !budget::charge(
                    result
                        .len()
                        .saturating_sub(start_offset)
                        .saturating_mul(core::mem::size_of::<ir::Node>()),
                    result.len().saturating_sub(start_offset),
                ) {
                    return error("");
                }
                let quantifee = result.split_off(start_offset);
                if self.loop_count as usize >= MAX_LOOPS {
                    return error("Loop count limit exceeded");
                }
                self.loop_count += 1;
                result.push(ir::Node::Loop {
                    loopee: Box::new(make_cat(quantifee)),
                    quant,
                    enclosed_groups: start_group..self.group_count,
                });
            }
        }
        Ok(make_cat(result))
    }

    fn try_consume_modifier_group(&mut self) -> Result<Option<ir::Node>, Error> {
        let mut cursor = self.input.clone();
        if cursor.next() != Some('(' as u32) {
            return Ok(None);
        }
        if cursor.next() != Some('?' as u32) {
            return Ok(None);
        }
        let Some(mut current) = cursor.next() else {
            return Ok(None);
        };

        if to_char_sat(current) == '<' {
            return Ok(None);
        }

        let mut seen_hyphen = false;
        let mut saw_flag = false;
        let mut icase_override: Option<bool> = None;
        let mut multiline_override: Option<bool> = None;
        let mut dot_all_override: Option<bool> = None;

        loop {
            let ch = to_char_sat(current);
            match ch {
                'i' | 'm' | 's' => {
                    let value = !seen_hyphen;
                    let target = match ch {
                        'i' => &mut icase_override,
                        'm' => &mut multiline_override,
                        's' => &mut dot_all_override,
                        _ => unreachable!(),
                    };
                    if target.is_some() {
                        return error("Invalid group modifier");
                    }
                    *target = Some(value);
                    saw_flag = true;
                }
                '-' => {
                    if seen_hyphen {
                        return error("Invalid group modifier");
                    }
                    seen_hyphen = true;
                }
                ':' => {
                    if !saw_flag {
                        return error("Invalid group modifier");
                    }
                    self.input = cursor;
                    let mut new_flags = self.flags;
                    if let Some(value) = icase_override {
                        new_flags.icase = value;
                    }
                    if let Some(value) = multiline_override {
                        new_flags.multiline = value;
                    }
                    if let Some(value) = dot_all_override {
                        new_flags.dot_all = value;
                    }
                    let saved_flags = self.flags;
                    self.flags = new_flags;
                    let contents = match self.consume_disjunction() {
                        Ok(node) => node,
                        Err(err) => {
                            self.flags = saved_flags;
                            return Err(err);
                        }
                    };
                    self.flags = saved_flags;
                    return Ok(Some(contents));
                }
                _ => {
                    return error("Invalid group modifier");
                }
            }

            current = match cursor.next() {
                Some(next) => next,
                None => return error("Invalid group modifier"),
            };
        }
    }

    /// ES6 21.2.2.13 CharacterClass.
    fn consume_bracket(&mut self) -> Result<ir::Node, Error> {
        self.consume('[');
        let invert = self.try_consume('^');
        let mut result = BracketContents {
            invert,
            cps: CodePointSet::default(),
        };

        loop {
            match self.peek().map(to_char_sat) {
                None => {
                    return error("Unbalanced bracket");
                }
                Some(']') => {
                    self.consume(']');
                    if self.flags.icase {
                        result.cps = if self.flags.unicode {
                            unicode::add_icase_code_points(result.cps)
                        } else {
                            unicode::add_legacy_icase_code_points(result.cps)
                        };
                    }
                    return Ok(ir::Node::Bracket(result));
                }
                _ => {}
            }

            // Parse a code point or character class.
            let Some(first) = self.try_consume_bracket_class_atom()? else {
                continue;
            };

            // Check for a dash; we may have a range.
            if !self.try_consume('-') {
                add_class_atom(&mut result, first);
                continue;
            }

            let Some(second) = self.try_consume_bracket_class_atom()? else {
                // No second atom. For example: [a-].
                add_class_atom(&mut result, first);
                add_class_atom(&mut result, ClassAtom::CodePoint(u32::from('-')));
                continue;
            };

            // Ranges must also be in order: z-a is invalid.
            // ES6 21.2.2.15.1 "If i > j, throw a SyntaxError exception"
            if let (ClassAtom::CodePoint(c1), ClassAtom::CodePoint(c2)) = (&first, &second) {
                if c1 > c2 {
                    return error(
                        "Range values reversed, start char code is greater than end char code.",
                    );
                }
                result.cps.add(Interval {
                    first: *c1,
                    last: *c2,
                });

                continue;
            }

            if self.flags.unicode {
                return error("Invalid character range");
            }

            // If it does not match a range treat as any match single characters.
            add_class_atom(&mut result, first);
            add_class_atom(&mut result, ClassAtom::CodePoint(u32::from('-')));
            add_class_atom(&mut result, second);
        }
    }

    fn try_consume_bracket_class_atom(&mut self) -> Result<Option<ClassAtom>, Error> {
        let c = self.peek();
        if c.is_none() {
            return Ok(None);
        }
        let c = c.unwrap();
        match to_char_sat(c) {
            // End of bracket.
            ']' => Ok(None),

            // ClassEscape
            '\\' => {
                self.consume('\\');
                let ec = if let Some(ec) = self.peek() {
                    ec
                } else {
                    return error("Unterminated escape");
                };
                match to_char_sat(ec) {
                    // ClassEscape :: b
                    'b' => {
                        self.consume('b');
                        Ok(Some(ClassAtom::CodePoint(u32::from('\x08'))))
                    }
                    // ClassEscape :: [+UnicodeMode] -
                    '-' if self.flags.unicode => {
                        self.consume('-');
                        Ok(Some(ClassAtom::CodePoint(u32::from('-'))))
                    }
                    'c' if !self.flags.unicode => {
                        let input = self.input.clone();
                        self.consume('c');
                        match self.peek().map(to_char_sat) {
                            // ClassEscape :: [~UnicodeMode] c ClassControlLetter
                            Some('0'..='9' | '_') => {
                                let next = self.next().expect("char was not next");
                                Ok(Some(ClassAtom::CodePoint(next & 0x1F)))
                            }
                            // CharacterEscape :: c AsciiLetter
                            Some('a'..='z' | 'A'..='Z') => {
                                let next = self.next().expect("char was not next");
                                Ok(Some(ClassAtom::CodePoint(next % 32)))
                            }
                            // ClassAtomNoDash :: \ [lookahead = c]
                            _ => {
                                self.input = input;
                                Ok(Some(ClassAtom::CodePoint(u32::from('\\'))))
                            }
                        }
                    }
                    // ClassEscape :: CharacterClassEscape :: d
                    'd' => {
                        self.consume('d');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Digits,
                            positive: true,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: D
                    'D' => {
                        self.consume('D');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Digits,
                            positive: false,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: s
                    's' => {
                        self.consume('s');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Spaces,
                            positive: true,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: S
                    'S' => {
                        self.consume('S');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Spaces,
                            positive: false,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: w
                    'w' => {
                        self.consume('w');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Words,
                            positive: true,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: W
                    'W' => {
                        self.consume('W');
                        Ok(Some(ClassAtom::CharacterClass {
                            class_type: CharacterClassType::Words,
                            positive: false,
                        }))
                    }
                    // ClassEscape :: CharacterClassEscape :: [+UnicodeMode] p{ UnicodePropertyValueExpression }
                    // ClassEscape :: CharacterClassEscape :: [+UnicodeMode] P{ UnicodePropertyValueExpression }
                    'p' | 'P' if self.flags.unicode => {
                        self.consume(ec);
                        let negate = ec == 'P' as u32;
                        match self.try_consume_unicode_property_escape()?.0 {
                            PropertyEscapeKind::CharacterClass(s) => Ok(Some(ClassAtom::Range {
                                iv: CodePointSet::from_sorted_disjoint_intervals(budget::copy(s)),
                                negate,
                            })),
                            PropertyEscapeKind::StringSet(_) => error("Invalid property escape"),
                        }
                    }
                    // ClassEscape :: CharacterEscape
                    _ => {
                        let cc = self.consume_character_escape()?;
                        Ok(Some(ClassAtom::CodePoint(cc)))
                    }
                }
            }

            _ => Ok(Some(ClassAtom::CodePoint(self.consume(c)))),
        }
    }

    // CharacterClass :: ClassContents :: ClassSetExpression
    fn consume_class_set_expression(&mut self, negate_set: bool) -> Result<ClassSet, Error> {
        let mut result = ClassSet::new();

        let first = match self.peek() {
            Some(0x5D /* ] */) => {
                self.consume(']');
                return Ok(result);
            }
            Some(_) => self.consume_class_set_operand(negate_set)?,
            None => {
                return error("Unbalanced class set bracket");
            }
        };

        enum ClassSetOperator {
            Union,
            Intersection,
            Subtraction,
        }

        let op = match self.peek() {
            Some(0x5D /* ] */) => {
                self.consume(']');
                result.union_operand(self.fold_set_operand(first));
                return Ok(result);
            }
            Some(0x26 /* & */) => {
                self.consume('&');
                if self.peek() == Some(0x26 /* & */) {
                    self.consume('&');
                    result.union_operand(self.fold_set_operand(first.clone()));
                    ClassSetOperator::Intersection
                } else {
                    result.codepoints.add_one(0x26 /* & */);
                    ClassSetOperator::Union
                }
            }
            Some(0x2D /* - */) => {
                self.consume('-');
                if self.peek() == Some(0x2D /* - */) {
                    self.consume('-');
                    result.union_operand(self.fold_set_operand(first.clone()));
                    ClassSetOperator::Subtraction
                } else {
                    match first {
                        ClassSetOperand::ClassSetCharacter(first) => {
                            let ClassSetOperand::ClassSetCharacter(last) =
                                self.consume_class_set_operand_raw(negate_set)?
                            else {
                                return error("Invalid class set range");
                            };
                            if first > last {
                                return error("Invalid class set range");
                            }
                            result.union_operand(self.fold_set_operand(
                                ClassSetOperand::CharacterClassEscape(
                                    CodePointSet::from_sorted_disjoint_intervals(vec![Interval {
                                        first,
                                        last,
                                    }]),
                                ),
                            ));
                        }
                        _ => {
                            return error("Invalid class set range");
                        }
                    };
                    ClassSetOperator::Union
                }
            }
            Some(_) => {
                result.union_operand(self.fold_set_operand(first.clone()));
                ClassSetOperator::Union
            }
            None => {
                return error("Unbalanced class set bracket");
            }
        };

        match op {
            ClassSetOperator::Union => {
                loop {
                    let operand = match self.peek() {
                        Some(0x5D /* ] */) => {
                            self.consume(']');
                            return Ok(result);
                        }
                        Some(_) => self.consume_class_set_operand(negate_set)?,
                        None => return error("Unbalanced class set bracket"),
                    };
                    if self.peek() == Some(0x2D /* - */) {
                        self.consume('-');
                        match operand {
                            ClassSetOperand::ClassSetCharacter(first) => {
                                let ClassSetOperand::ClassSetCharacter(last) =
                                    self.consume_class_set_operand_raw(negate_set)?
                                else {
                                    return error("Invalid class set range");
                                };
                                if first > last {
                                    return error("Invalid class set range");
                                }
                                result.union_operand(self.fold_set_operand(
                                    ClassSetOperand::CharacterClassEscape(
                                        CodePointSet::from_sorted_disjoint_intervals(vec![
                                            Interval { first, last },
                                        ]),
                                    ),
                                ));
                            }
                            _ => {
                                return error("Invalid class set range");
                            }
                        };
                    } else {
                        result.union_operand(self.fold_set_operand(operand));
                    }
                }
            }
            // ClassIntersection :: ClassSetOperand && [lookahead ≠ &]
            ClassSetOperator::Intersection => {
                loop {
                    let operand = self.consume_class_set_operand(negate_set)?;
                    result.intersect_operand(self.fold_set_operand(operand));
                    match self.next() {
                        Some(0x5D /* ] */) => return Ok(result),
                        Some(0x26 /* & */) => {}
                        Some(_) => return error("Unexpected character in class set intersection"),
                        _ => return error("Unbalanced class set bracket"),
                    }
                    if self.next() != Some(0x26 /* & */) {
                        return error("Unbalanced class set bracket");
                    }
                }
            }
            // ClassSubtraction :: ClassSubtraction -- ClassSetOperand
            ClassSetOperator::Subtraction => {
                loop {
                    let operand = self.consume_class_set_operand(negate_set)?;
                    result.subtract_operand(self.fold_set_operand(operand));
                    match self.next() {
                        Some(0x5D /* ] */) => return Ok(result),
                        Some(0x2D /* - */) => {}
                        Some(_) => return error("Unexpected character in class set subtraction"),
                        _ => return error("Unbalanced class set bracket"),
                    }
                    if self.next() != Some(0x2D /* - */) {
                        return error("Unbalanced class set bracket");
                    }
                }
            }
        }
    }

    fn consume_class_set_operand(&mut self, negate_set: bool) -> Result<ClassSetOperand, Error> {
        if !budget::charge(core::mem::size_of::<Interval>(), 1) {
            return error("");
        }
        self.consume_class_set_operand_raw(negate_set)
    }
    fn fold_set_operand(&self, operand: ClassSetOperand) -> ClassSetOperand {
        if !self.flags.icase || matches!(operand, ClassSetOperand::Class(_)) {
            return operand;
        }
        let mut class = ClassSet::new();
        class.union_operand(operand);
        class.codepoints = unicode::add_icase_code_points(class.codepoints);
        for text in &mut class.alternatives.0 {
            for cp in text {
                *cp = unicode::fold(*cp);
            }
        }
        ClassSetOperand::Class(class)
    }
    fn consume_class_set_operand_raw(
        &mut self,
        negate_set: bool,
    ) -> Result<ClassSetOperand, Error> {
        use ClassSetOperand::*;
        let Some(cp) = self.peek() else {
            return error("Empty class set operand");
        };
        match cp {
            // ClassSetOperand :: NestedClass :: [ [lookahead ≠ ^] ClassContents[+UnicodeMode, +UnicodeSetsMode] ]
            // ClassSetOperand :: NestedClass :: [^ ClassContents[+UnicodeMode, +UnicodeSetsMode] ]
            0x5B /* [ */ => {
                self.consume('[');
                let negate_set = self.try_consume('^');
                let mut result = self.consume_class_set_expression(negate_set)?;
                if negate_set {
                    result.codepoints = result.codepoints.inverted();
                }
                Ok(Class(result))
            }
            // ClassSetOperand :: NestedClass :: \ CharacterClassEscape
            // ClassSetOperand :: ClassStringDisjunction
            // ClassSetOperand :: ClassSetCharacter :: \...
            // ClassSetRange :: ClassSetCharacter :: \...
            0x5C /* \ */ => {
                self.consume('\\');
                let Some(cp) = self.peek() else {
                    return error("Incomplete class set escape");
                };
                match cp {
                    // ClassStringDisjunction  \q{ ClassStringDisjunctionContents }
                    0x71 /* q */ => {
                        self.consume('q');
                        if !self.try_consume('{') {
                            return error("Invalid class set escape: expected {");
                        }
                        let mut alternatives = Vec::new();
                        let mut alternative = Vec::new();
                        loop {
                            match self.peek() {
                                Some(0x7D /* } */) => {
                                    self.consume('}');
                                    if !budget::reserve(&mut alternatives,1){return error("");}
                                    alternatives.push(core::mem::take(&mut alternative));
                                    break;
                                }
                                Some(0x7C /* | */) => {
                                    self.consume('|');
                                    if !budget::reserve(&mut alternatives,1){return error("");}
                                    alternatives.push(core::mem::take(&mut alternative));
                                }
                                Some(_) => {
                                    if !budget::reserve(&mut alternative,1){return error("");}
                                    alternative.push(self.consume_class_set_character()?);
                                }
                                None => {
                                    return error("Unbalanced class set string disjunction");
                                }
                            }
                        }
                        let mut class=ClassSet::new();
                        for mut text in alternatives {
                            if self.flags.icase {for cp in &mut text {*cp=unicode::fold(*cp);}}
                            if text.len()==1 {class.codepoints.add_one(text[0]);}
                            else {if negate_set{return error("Negated class cannot contain strings");}if !budget::push(&mut class.alternatives.0,text){return error("");}}
                        }
                        Ok(Class(class))
                    }
                    // CharacterClassEscape :: d
                    0x64 /* d */ => {
                        self.consume('d');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Digits, true)))
                    }
                    // CharacterClassEscape :: D
                    0x44 /* D */ => {
                        self.consume('D');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Digits, false)))
                    }
                    // CharacterClassEscape :: s
                    0x73 /* s */ => {
                        self.consume('s');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Spaces, true)))
                    }
                    // CharacterClassEscape :: S
                    0x53 /* S */ => {
                        self.consume('S');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Spaces, false)))
                    }
                    // CharacterClassEscape :: w
                    0x77 /* w */ => {
                        self.consume('w');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Words, true)))
                    }
                    // CharacterClassEscape :: W
                    0x57 /* W */ => {
                        self.consume('W');
                        Ok(CharacterClassEscape(codepoints_from_class(CharacterClassType::Words, false)))
                    }
                    // CharacterClassEscape :: [+UnicodeMode] p{ UnicodePropertyValueExpression }
                    0x70 /* p */ => {
                        self.consume('p');
                        match self.try_consume_unicode_property_escape()? {
                            (PropertyEscapeKind::CharacterClass(intervals), special) => {
                                let mut class=ClassSet::new();
                                class.codepoints=CodePointSet::from_sorted_disjoint_intervals(budget::copy(intervals));
                                if self.flags.icase && !special {class.codepoints=unicode::add_icase_code_points(class.codepoints);}
                                Ok(Class(class))
                            }
                            (PropertyEscapeKind::StringSet(_), _) if negate_set => error("Invalid character escape"),
                            (PropertyEscapeKind::StringSet(strings), _) => {
                                let mut result=Vec::new();
                                if !budget::reserve(&mut result,strings.len()){return error("");}
                                for s in strings {result.push(budget::copy(s));if budget::failed(){return error("");}}
                                Ok(ClassStringDisjunction(ClassSetAlternativeStrings(result)))
                            }
                        }
                    }
                    // CharacterClassEscape :: [+UnicodeMode] P{ UnicodePropertyValueExpression }
                    0x50 /* P */ => {
                        self.consume('P');
                        match self.try_consume_unicode_property_escape()? {
                            (PropertyEscapeKind::CharacterClass(s), special) => {
                                let mut class=ClassSet::new();
                                class.codepoints=CodePointSet::from_sorted_disjoint_intervals(budget::copy(s));
                                if self.flags.icase && !special {class.codepoints=unicode::add_icase_code_points(class.codepoints);}
                                class.codepoints=class.codepoints.inverted();
                                Ok(Class(class))
                            }
                            (PropertyEscapeKind::StringSet(_), _) => error("Invalid character escape"),
                        }
                    }
                    // ClassSetCharacter:: \b
                    0x62 /* b */ => {
                        Ok(ClassSetCharacter(self.consume(cp)))
                    }
                    // ClassSetCharacter:: \ ClassSetReservedPunctuator
                    _ if Self::is_class_set_reserved_punctuator(cp) => Ok(ClassSetCharacter(self.consume(cp))),
                    // ClassSetCharacter:: \ CharacterEscape[+UnicodeMode]
                    _ => Ok(ClassSetCharacter(self.consume_character_escape()?))
                }
            }
            // ClassSetOperand :: ClassSetCharacter
            // ClassSetRange :: ClassSetCharacter
            _ => Ok(ClassSetCharacter(self.consume_class_set_character()?)),
        }
    }

    // ClassSetCharacter
    fn consume_class_set_character(&mut self) -> Result<u32, Error> {
        let Some(cp) = self.next() else {
            return error("Incomplete class set character");
        };
        match cp {
            0x5C /* \ */ => {
                let Some(cp) = self.peek() else {
                    return error("Incomplete class set escape");
                };
                match cp {
                    // \b
                    0x62 /* b */ => {
                        Ok(self.consume(cp))
                    }
                    // \ ClassSetReservedPunctuator
                    _ if Self::is_class_set_reserved_punctuator(cp) => Ok(self.consume(cp)),
                    // \ CharacterEscape[+UnicodeMode]
                    _ => Ok(self.consume_character_escape()?)
                }
            }
            // [lookahead ∉ ClassSetReservedDoublePunctuator] SourceCharacter but not ClassSetSyntaxCharacter
            0x28 /* ( */ | 0x29 /* ) */ | 0x7B /* { */ | 0x7D /* } */ | 0x2F /* / */
            | 0x2D /* - */ | 0x7C /* | */ => error("Invalid class set character"),
            _ => {
                if Self::is_class_set_reserved_double_punctuator(cp)
                    && let Some(cp) = self.peek()
                        && Self::is_class_set_reserved_double_punctuator(cp) {
                            return error("Invalid class set character");
                        }
                Ok(cp)
            }
        }
    }

    // ClassSetReservedPunctuator
    fn is_class_set_reserved_punctuator(cp: u32) -> bool {
        match cp {
            0x26 /* & */ | 0x2D /* - */ | 0x21 /* ! */ | 0x23 /* # */ | 0x25 /* % */
            | 0x2C /* , */ | 0x3A /* : */ | 0x3B /* ; */ | 0x3C /* < */ | 0x3D /* = */
            | 0x3E /* > */ | 0x40 /* @ */ | 0x60 /* ` */ | 0x7E /* ~ */ => true,
            _ => false,
        }
    }

    fn is_class_set_reserved_double_punctuator(cp: u32) -> bool {
        match cp {
            0x26 /* & */ | 0x21 /* ! */ | 0x23 /* # */ | 0x24 /* $ */ | 0x25 /* % */
            | 0x2A /* * */ | 0x2B /* + */ | 0x2C /* , */ | 0x2E /* . */ | 0x3A /* : */
            | 0x3B /* ; */ | 0x3C /* < */ | 0x3D /* = */ | 0x3E /* > */ | 0x3F /* ? */
            | 0x40 /* @ */ | 0x5E /* ^ */ | 0x60 /* ` */ | 0x7E /* ~ */ => true,
            _ => false,
        }
    }

    fn try_consume_quantifier(&mut self) -> Result<Option<ir::Quantifier>, Error> {
        if let Some(mut quant) = self.try_consume_quantifier_prefix()? {
            quant.greedy = !self.try_consume('?');
            Ok(Some(quant))
        } else {
            Ok(None)
        }
    }

    fn try_consume_quantifier_prefix(&mut self) -> Result<Option<ir::Quantifier>, Error> {
        let nc = self.peek();
        if nc.is_none() {
            return Ok(None);
        }
        let c = nc.unwrap();
        match char::from_u32(c) {
            Some('+') => {
                self.consume('+');
                Ok(Some(ir::Quantifier {
                    min: 1,
                    max: None,
                    greedy: true,
                }))
            }
            Some('*') => {
                self.consume('*');
                Ok(Some(ir::Quantifier {
                    min: 0,
                    max: None,
                    greedy: true,
                }))
            }
            Some('?') => {
                self.consume('?');
                Ok(Some(ir::Quantifier {
                    min: 0,
                    max: Some(1),
                    greedy: true,
                }))
            }
            Some('{') => {
                if let Some(quantifier) = self.try_consume_braced_quantifier() {
                    Ok(Some(quantifier))
                } else if self.flags.unicode {
                    // if there was a brace '{' that doesn't parse into a valid quantifier,
                    // it's not valid with the unicode flag
                    error("Invalid quantifier")
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn try_consume_braced_quantifier(&mut self) -> Option<ir::Quantifier> {
        // if parsed input is actually invalid, keep the previous one for rollback
        let pre_input = self.input.clone();
        self.consume('{');
        let optmin = self.try_consume_decimal_integer_literal();
        let Some(optmin) = optmin else {
            // not a valid quantifier, rollback consumption
            self.input = pre_input;
            return None;
        };
        let mut quant = ir::Quantifier {
            min: optmin,
            max: Some(optmin),
            greedy: true,
        };
        if self.try_consume(',') {
            let max = self.try_consume_decimal_integer_literal();
            // Either like {3,4} in which case we want to set the max;
            // or like {3,} in which case the max should be None to indicate unbounded.
            quant.max = max;
        } else {
            // Like {3}.
        }
        if !self.try_consume('}') {
            // not a valid quantifier, rollback consumption
            self.input = pre_input;
            return None;
        }
        Some(quant)
    }

    /// ES6 11.8.3 DecimalIntegerLiteral.
    /// If the value would overflow, usize::MAX is returned.
    /// All decimal digits are consumed regardless.
    fn try_consume_decimal_integer_literal(&mut self) -> Option<usize> {
        let mut result: usize = 0;
        let mut char_count = 0;
        while let Some(c) = self.peek() {
            if let Some(digit) = char::from_u32(c).and_then(|c| char::to_digit(c, 10)) {
                self.consume(c);
                char_count += 1;
                result = result.saturating_mul(10);
                result = result.saturating_add(digit as usize);
            } else {
                break;
            }
        }
        if char_count > 0 { Some(result) } else { None }
    }

    fn consume_lookaround_assertion(
        &mut self,
        params: LookaroundParams,
    ) -> Result<ir::Node, Error> {
        let start_group = self.group_count;
        let contents = self.consume_disjunction()?;
        let end_group = self.group_count;
        Ok(ir::Node::LookaroundAssertion {
            negate: params.negate,
            backwards: params.backwards,
            start_group,
            end_group,
            contents: Box::new(contents),
        })
    }

    fn consume_character_escape(&mut self) -> Result<u32, Error> {
        let c = self.next().expect("Should have a character");
        let ch = to_char_sat(c);
        match ch {
            // CharacterEscape :: ControlEscape :: f
            'f' => Ok(0xC),
            // CharacterEscape :: ControlEscape :: n
            'n' => Ok(0xA),
            // CharacterEscape :: ControlEscape :: r
            'r' => Ok(0xD),
            // CharacterEscape :: ControlEscape :: t
            't' => Ok(0x9),
            // CharacterEscape :: ControlEscape :: v
            'v' => Ok(0xB),
            // CharacterEscape :: c AsciiLetter
            'c' => {
                if let Some(nc) = self.next().and_then(char::from_u32)
                    && (nc.is_ascii_lowercase() || nc.is_ascii_uppercase())
                {
                    return Ok((nc as u32) % 32);
                }
                error("Invalid character escape")
            }
            // CharacterEscape :: 0 [lookahead ∉ DecimalDigit]
            '0' if self
                .peek()
                .and_then(char::from_u32)
                .map(|c: char| c.is_ascii_digit())
                != Some(true) =>
            {
                Ok(0x0)
            }
            // CharacterEscape :: HexEscapeSequence :: x HexDigit HexDigit
            'x' => {
                let hex_to_digit = |c: char| c.to_digit(16);
                let x1 = self.next().and_then(char::from_u32).and_then(hex_to_digit);
                let x2 = self.next().and_then(char::from_u32).and_then(hex_to_digit);
                match (x1, x2) {
                    (Some(x1), Some(x2)) => Ok(x1 * 16 + x2),
                    // CharacterEscape :: IdentityEscape :: SourceCharacterIdentityEscape
                    _ if !self.flags.unicode => Ok(c),
                    _ => error("Invalid character escape"),
                }
            }
            // CharacterEscape :: RegExpUnicodeEscapeSequence
            'u' => {
                if let Some(c) = self.try_escape_unicode_sequence() {
                    Ok(c)
                } else if !self.flags.unicode {
                    // CharacterEscape :: IdentityEscape :: SourceCharacterIdentityEscape
                    Ok(c)
                } else {
                    error("Invalid unicode escape")
                }
            }
            // CharacterEscape :: [~UnicodeMode] LegacyOctalEscapeSequence
            '0'..='7' if !self.flags.unicode => {
                let Some(c1) = self.peek() else {
                    return Ok(c - '0' as u32);
                };
                let ch1 = to_char_sat(c1);

                match ch {
                    // 0 [lookahead ∈ { 8, 9 }]
                    '0' if ('8'..='9').contains(&ch1) => Ok(0x0),
                    // NonZeroOctalDigit [lookahead ∉ OctalDigit]
                    _ if !('0'..='7').contains(&ch1) => Ok(c - '0' as u32),
                    // FourToSeven OctalDigit
                    '4'..='7' => {
                        self.consume(c1);
                        Ok((c - '0' as u32) * 8 + c1 - '0' as u32)
                    }
                    // ZeroToThree OctalDigit [lookahead ∉ OctalDigit]
                    // ZeroToThree OctalDigit OctalDigit
                    '0'..='3' => {
                        self.consume(c1);
                        if self.peek().map(|c2| ('0'..='7').contains(&to_char_sat(c2)))
                            == Some(true)
                        {
                            let c2 = self.next().expect("char was not next");
                            Ok((c - '0' as u32) * 64 + (c1 - '0' as u32) * 8 + c2 - '0' as u32)
                        } else {
                            Ok((c - '0' as u32) * 8 + c1 - '0' as u32)
                        }
                    }
                    _ => unreachable!(),
                }
            }
            // CharacterEscape :: IdentityEscape :: [+UnicodeMode] SyntaxCharacter
            // CharacterEscape :: IdentityEscape :: [+UnicodeMode] /
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
            | '/' => Ok(c),
            // CharacterEscape :: IdentityEscape :: SourceCharacterIdentityEscape
            _ if !self.flags.unicode => Ok(c),
            _ => error("Invalid character escape"),
        }
    }

    // AtomEscape
    fn consume_atom_escape(&mut self) -> Result<ir::Node, Error> {
        let Some(c) = self.peek() else {
            return error("Incomplete escape");
        };
        match to_char_sat(c) {
            'd' | 'D' => {
                self.consume(c);
                Ok(make_bracket_class(
                    CharacterClassType::Digits,
                    c == 'd' as u32,
                    self.flags.icase,
                    self.flags.unicode,
                ))
            }

            's' | 'S' => {
                self.consume(c);
                Ok(make_bracket_class(
                    CharacterClassType::Spaces,
                    c == 's' as u32,
                    self.flags.icase,
                    self.flags.unicode,
                ))
            }

            'w' | 'W' => {
                self.consume(c);
                Ok(make_bracket_class(
                    CharacterClassType::Words,
                    c == 'w' as u32,
                    self.flags.icase,
                    self.flags.unicode,
                ))
            }

            // ClassEscape :: CharacterClassEscape :: [+UnicodeMode] p{ UnicodePropertyValueExpression }
            // ClassEscape :: CharacterClassEscape :: [+UnicodeMode] P{ UnicodePropertyValueExpression }
            'p' | 'P' if self.flags.unicode || self.flags.unicode_sets => {
                self.consume(c);
                let negate = c == 'P' as u32;
                let (property_escape, special) = self.try_consume_unicode_property_escape()?;
                match property_escape {
                    PropertyEscapeKind::CharacterClass(s) => {
                        let mut cps = CodePointSet::from_sorted_disjoint_intervals(budget::copy(s));
                        if self.flags.unicode_sets {
                            if self.flags.icase && !special {
                                cps = unicode::add_icase_code_points(cps);
                            }
                            if negate {
                                cps = cps.inverted();
                            }
                        } else {
                            if negate {
                                cps = cps.inverted();
                            }
                            if self.flags.icase {
                                cps = unicode::add_icase_code_points(cps);
                            }
                        }
                        Ok(ir::Node::Bracket(BracketContents { invert: false, cps }))
                    }
                    PropertyEscapeKind::StringSet(_) if negate => error("Invalid character escape"),
                    PropertyEscapeKind::StringSet(strings) => {
                        let mut values = Vec::new();
                        if !budget::reserve(&mut values, strings.len()) {
                            return error("");
                        }
                        for string in strings {
                            values.push(budget::copy(string));
                            if budget::failed() {
                                return error("");
                            }
                        }
                        Ok(make_alt(
                            ClassSetAlternativeStrings(values).to_nodes(self.flags.icase),
                        ))
                    }
                }
            }

            // [+UnicodeMode] DecimalEscape
            // Note: This is a backreference.
            '1'..='9' if self.flags.unicode => {
                let group = self.try_consume_decimal_integer_literal().unwrap();
                if group <= self.group_count_max as usize {
                    Ok(ir::Node::BackRef(group as u32))
                } else {
                    error("Invalid character escape")
                }
            }

            // [~UnicodeMode] DecimalEscape but only if the CapturingGroupNumber of DecimalEscape
            //    is ≤ CountLeftCapturingParensWithin(the Pattern containing DecimalEscape)
            // Note: This could be either a backreference, a legacy octal escape or an identity escape.
            '1'..='9' => {
                let input = self.input.clone();
                let group = self.try_consume_decimal_integer_literal().unwrap();

                if group <= self.group_count_max as usize {
                    Ok(ir::Node::BackRef(group as u32))
                } else {
                    self.input = input;
                    let c = self.consume_character_escape()?;
                    Ok(ir::Node::Char {
                        c: self.fold_if_icase(c),
                        icase: self.flags.icase,
                    })
                }
            }

            // [+NamedCaptureGroups] k GroupName
            'k' if self.flags.unicode || !self.named_group_indices.is_empty() => {
                // The sequence `\k` must be the start of a backreference to a named capture group.
                // Note multiple capture groups may have the same name; we must map all of them to their indices.
                self.consume('k');
                // Must have a valid group name.
                let Some(group_name) = self.try_consume_named_capture_group_name() else {
                    return error("Invalid named backreference syntax");
                };
                // The group name must be the name of a previously defined capture group.
                let Some(group_indices) = self.named_group_indices.get(&group_name) else {
                    return error_join(
                        "Backreference to invalid named capture group: ",
                        &group_name,
                    );
                };
                // Note backreferences are 1-based.
                let node = match group_indices.len() {
                    0 => unreachable!("Should not have empty indices for group name"),
                    1 => {
                        // Common case of a backref matching a single group.
                        ir::Node::BackRef(group_indices[0] + 1)
                    }
                    _ => {
                        // Unusual case of multiple groups sharing a name: the backref should try each in turn.
                        // Lower to alternations of backreferences. Reverse to keep it right-associative: a | (b | (c | d))...
                        if !budget::charge(
                            group_indices
                                .len()
                                .saturating_mul(2)
                                .saturating_mul(core::mem::size_of::<ir::Node>()),
                            group_indices.len(),
                        ) {
                            return error("");
                        }
                        let backrefs = group_indices
                            .iter()
                            .rev()
                            .map(|group_index| ir::Node::BackRef(*group_index + 1));
                        backrefs
                            .reduce(|right, left| ir::Node::Alt(Box::new(left), Box::new(right)))
                            .unwrap()
                    }
                };
                Ok(node)
            }

            // [~NamedCaptureGroups] k GroupName
            'k' => {
                self.consume('k');
                Ok(ir::Node::Char {
                    c: self.fold_if_icase(c),
                    icase: self.flags.icase,
                })
            }

            _ => {
                let c = self.consume_character_escape()?;
                Ok(ir::Node::Char {
                    c: self.fold_if_icase(c),
                    icase: self.flags.icase,
                })
            }
        }
    }

    #[allow(clippy::branches_sharing_code)]
    fn try_escape_unicode_sequence(&mut self) -> Option<u32> {
        let mut orig_input = self.input.clone();

        // Support \u{X..X} (Unicode CodePoint)
        if self.try_consume('{') {
            let mut s = String::new();
            loop {
                match self.next().and_then(char::from_u32) {
                    Some('}') => break,
                    Some(c) => {
                        if !budget::push_char(&mut s, c) {
                            return None;
                        }
                    }
                    None => {
                        // Surrogates not supported in code point escapes.
                        self.input = orig_input;
                        return None;
                    }
                }
            }

            match u32::from_str_radix(&s, 16) {
                Ok(u) => {
                    if u > 0x10_FFFF {
                        self.input = orig_input;
                        None
                    } else {
                        Some(u)
                    }
                }
                _ => {
                    self.input = orig_input;
                    None
                }
            }
        } else {
            // Hex4Digits
            let mut s = String::new();
            for _ in 0..4 {
                if let Some(c) = self.next().and_then(char::from_u32) {
                    if !budget::push_char(&mut s, c) {
                        return None;
                    }
                } else {
                    // Surrogates are not hex digits.
                    self.input = orig_input;
                    return None;
                }
            }
            match u16::from_str_radix(&s, 16) {
                Ok(u) => {
                    if (0xD800..=0xDBFF).contains(&u) {
                        // Found a high surrogate. Try to parse a low surrogate next
                        // to see if we can rebuild the original `char`

                        if !self.try_consume_str("\\u") {
                            return Some(u as u32);
                        }
                        orig_input = self.input.clone();

                        // A poor man's try block to handle the backtracking
                        // in a single place instead of every time we want to return.
                        // This allows us to use `?` within the inner block without returning
                        // from the entire parent function.
                        let result = (|| {
                            let mut s = String::new();
                            for _ in 0..4 {
                                let c = self.next().and_then(char::from_u32)?;
                                if !budget::push_char(&mut s, c) {
                                    return None;
                                }
                            }

                            let uu = u16::from_str_radix(&s, 16).ok()?;
                            let ch = char::decode_utf16([u, uu]).next()?.ok()?;
                            Some(u32::from(ch))
                        })();

                        result.or_else(|| {
                            self.input = orig_input;
                            Some(u as u32)
                        })
                    } else {
                        // If `u` is not a surrogate or is a low surrogate we can directly return it,
                        // since all paired low surrogates should have been handled above.
                        Some(u as u32)
                    }
                }
                _ => {
                    self.input = orig_input;
                    None
                }
            }
        }
    }

    fn try_consume_named_capture_group_name(&mut self) -> Option<String> {
        if !self.try_consume('<') {
            return None;
        }

        let orig_input = self.input.clone();
        let mut group_name = String::new();

        if let Some(mut c) = self.next().and_then(char::from_u32) {
            if c == '\\' && self.try_consume('u') {
                if let Some(escaped) = self.try_escape_unicode_sequence().and_then(char::from_u32) {
                    c = escaped;
                } else {
                    self.input = orig_input;
                    return None;
                }
            }

            if interval_contains(id_start_ranges(), c.into()) || c == '$' || c == '_' {
                if !budget::charge(group_name.len().saturating_add(c.len_utf8()), 1) {
                    return None;
                }
                if !budget::push_char(&mut group_name, c) {
                    return None;
                }
            } else {
                self.input = orig_input;
                return None;
            }
        } else {
            self.input = orig_input;
            return None;
        }

        loop {
            if let Some(mut c) = self.next().and_then(char::from_u32) {
                if c == '\\' && self.try_consume('u') {
                    if let Some(escaped) =
                        self.try_escape_unicode_sequence().and_then(char::from_u32)
                    {
                        c = escaped;
                    } else {
                        self.input = orig_input;
                        return None;
                    }
                }

                if c == '>' {
                    break;
                }

                if interval_contains(id_continue_ranges(), c.into()) || c == '$' || c == '_' || c == '\u{200C}' /* <ZWNJ> */ || c == '\u{200D}'
                /* <ZWJ> */
                {
                    if !budget::charge(group_name.len().saturating_add(c.len_utf8()), 1) {
                        return None;
                    }
                    if !budget::push_char(&mut group_name, c) {
                        return None;
                    }
                } else {
                    self.input = orig_input;
                    return None;
                }
            } else {
                self.input = orig_input;
                return None;
            }
        }

        Some(group_name)
    }

    // Quickly parse all capture groups.
    // Per TC39 proposal, duplicate named groups are allowed in different alternatives.
    fn parse_capture_groups(&mut self) -> Result<(), Error> {
        let orig_input = self.input.clone();

        // Pass 1: Collect all named capture groups with their alternative paths
        let named_group_locations = self.collect_named_group_locations()?;

        // Pass 2: Check for conflicts (duplicates in the same alternative path)
        self.check_duplicate_conflicts(&named_group_locations)?;

        self.input = orig_input;

        Ok(())
    }

    /// Pass 1: Collect all named capture groups and record which alternative path each appears in.
    fn collect_named_group_locations(
        &mut self,
    ) -> Result<ParserMap<String, Vec<AlternativePath>>, Error> {
        // Track parenthesis depth and alternative index at each depth
        let mut paren_depth: usize = 0;
        // Map from depth to current alternative index at that depth
        let mut alt_indices: ParserMap<usize, usize> = ParserMap::new();
        alt_indices.insert(0, 0);

        // Map from group name to all alternative paths where it appears
        let mut named_group_locations: ParserMap<String, Vec<AlternativePath>> = ParserMap::new();

        loop {
            match self.next().map(to_char_sat) {
                Some('\\') => {
                    self.next();
                    continue;
                }
                Some('[') => loop {
                    match self.next().map(to_char_sat) {
                        Some('\\') => {
                            self.next();
                            continue;
                        }
                        Some(']') => break,
                        Some(_) => continue,
                        None => break,
                    }
                },
                Some('(') => {
                    // Determine whether we're a capturing group, and optionally the name.
                    let is_capturing;
                    let group_name;
                    if self.try_consume_str("?") {
                        group_name = self.try_consume_named_capture_group_name();
                        is_capturing = group_name.is_some(); // (?:, (?=, (?!, etc. are non-capturing.
                    } else {
                        is_capturing = true;
                        group_name = None; // Unnamed capture group
                    }

                    if let Some(name) = group_name {
                        // Build current alternative path from depth 0 to current depth.
                        let mut segments = Vec::new();
                        if !budget::reserve(&mut segments, paren_depth.saturating_add(1)) {
                            return error("");
                        }
                        for d in 0..=paren_depth {
                            segments.push((d, *alt_indices.get(&d).unwrap_or(&0)));
                        }

                        if !budget::charge(name.len(), name.len()) {
                            return error("");
                        }
                        let Some(paths) =
                            named_group_locations.get_or_insert(name.clone(), Vec::new())
                        else {
                            return error("");
                        };
                        if !budget::push(paths, AlternativePath { segments }) {
                            return error("");
                        }
                        let Some(indices) =
                            self.named_group_indices.get_or_insert(name, Vec::new())
                        else {
                            return error("");
                        };
                        if !budget::push(indices, self.group_count_max) {
                            return error("");
                        }
                    }

                    if is_capturing {
                        self.group_count_max =
                            if self.group_count_max + 1 > MAX_CAPTURE_GROUPS as u32 {
                                MAX_CAPTURE_GROUPS as u32
                            } else {
                                self.group_count_max + 1
                            };
                    }

                    // Entering a new group.
                    paren_depth += 1;
                    if !alt_indices.insert(paren_depth, 0) {
                        return error("");
                    }
                }
                Some(')') => {
                    // Exiting a group
                    if paren_depth > 0 {
                        alt_indices.remove(&paren_depth);
                        paren_depth -= 1;
                    }
                }
                Some('|') => {
                    // Moving to next alternative at current depth
                    let Some(index) = alt_indices.get_or_insert(paren_depth, 0) else {
                        return error("");
                    };
                    *index = index.checked_add(1).ok_or_else(|| Error {
                        text: String::new(),
                    })?;
                }
                Some(_) => continue,
                None => break,
            }
        }

        Ok(named_group_locations)
    }

    /// Pass 2: Check that named groups with the same name don't conflict.
    /// Groups conflict if they appear in the same alternative path.
    /// Per TC39 proposal, `/(?<a>x)|(?<a>y)/` is valid (different alternatives),
    /// but `/(?<a>x)(?<a>y)/` is invalid (same alternative).
    fn check_duplicate_conflicts(
        &self,
        named_group_locations: &ParserMap<String, Vec<AlternativePath>>,
    ) -> Result<(), Error> {
        if !budget::charge(core::mem::size_of::<usize>().saturating_mul(4), 1) {
            return error("");
        }
        for paths in named_group_locations.values() {
            if !budget::charge(0, paths.len().saturating_add(1)) {
                return error("");
            }
            // Check each pair of paths for this group name
            for i in 0..paths.len() {
                for j in (i + 1)..paths.len() {
                    if !budget::charge(
                        0,
                        paths[i]
                            .segments
                            .len()
                            .saturating_mul(paths[j].segments.len())
                            .saturating_add(1),
                    ) {
                        return error("");
                    }
                    if paths[i].conflicts_with(&paths[j]) {
                        return error("Duplicate capture group name");
                    }
                }
            }
        }
        Ok(())
    }

    fn try_consume_unicode_property_escape(&mut self) -> Result<(PropertyEscapeKind, bool), Error> {
        if !self.try_consume('{') {
            return error("Invalid character at property escape start");
        }

        let mut buffer = String::new();
        let mut name = None;

        while let Some(c) = self.peek().and_then(char::from_u32) {
            match c {
                '}' => {
                    self.consume(c);
                    let Some(value) =
                        unicode_property_from_str(&buffer, name, self.flags.unicode_sets)
                    else {
                        break;
                    };
                    return Ok((
                        value,
                        name.is_none() && matches!(buffer.as_str(), "ASCII" | "Any"),
                    ));
                }
                '=' if name.is_none() => {
                    self.consume(c);
                    let Some(n) = unicode_property_name_from_str(&buffer) else {
                        break;
                    };
                    name = Some(n);
                    buffer.clear();
                }
                c if c.is_ascii_alphanumeric() || c == '_' => {
                    self.consume(c);
                    if !budget::push_char(&mut buffer, c) {
                        return error("");
                    }
                }
                _ => break,
            }
        }

        error("Invalid property name")
    }

    fn finalize(&self, mut re: ir::Regex) -> Result<ir::Regex, Error> {
        debug_assert!(self.loop_count <= MAX_LOOPS as u32);
        debug_assert!(self.group_count as usize <= MAX_CAPTURE_GROUPS);
        if self.has_lookbehind {
            ir::walk_mut(
                false,
                re.flags.unicode,
                &mut re.node,
                &mut ir::Node::reverse_cats,
            );
        }
        Ok(re)
    }
}

/// Try parsing a given pattern.
/// Return the resulting IR regex, or an error.
pub fn try_parse<I>(pattern: I, flags: api::Flags) -> Result<ir::Regex, Error>
where
    I: Iterator<Item = u32> + Clone,
{
    let mut p = Parser {
        input: pattern.peekable(),
        flags,
        loop_count: 0,
        group_count: 0,
        named_group_indices: ParserMap::new(),
        group_count_max: 0,
        has_lookbehind: false,
    };
    p.try_parse()
}
