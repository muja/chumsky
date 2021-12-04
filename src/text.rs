use super::*;
use core::iter::FromIterator;

/// The type of a parser that accepts (and ignores) any number of whitespace characters.
pub type Padding<I, E> = Custom<fn(&mut StreamOf<I, E>) -> PResult<I, (), E>, E>;

/// The type of a parser that accepts (and ignores) any number of whitespace characters before or after another
/// pattern.
pub type Padded<P, I, O> = ThenIgnore<
    IgnoreThen<Padding<I, <P as Parser<I, O>>::Error>, P, (), O>,
    Padding<I, <P as Parser<I, O>>::Error>,
    O,
    (),
>;

mod private {
    pub trait Sealed {}

    impl Sealed for u8 {}
    impl Sealed for char {}
}

/// A trait implemented by textual character types (currently, [`u8`] and [`char`]).
///
/// Avoid implementing this trait yourself if you can: it's *very* likely to be expanded in future versions!
pub trait Character: private::Sealed + Copy + PartialEq {
    /// The default unsized [`str`]-like type of a linear sequence of this character.
    type Str: ?Sized + PartialEq;
    /// The default type that this character collects into.
    type Collection: Chain<Self> + FromIterator<Self> + AsRef<Self::Str> + 'static;

    /// Returns true if the character is canonically considered to be whitespace.
    fn is_whitespace(&self) -> bool;

    /// Return the '0' digit of the character.
    fn digit_zero() -> Self;

    /// Returns true if the character is canonically considered to be a numeric digit.
    fn is_digit(&self, radix: u32) -> bool;

    /// Returns this character as a [`char`].
    fn to_char(&self) -> char;
}

impl Character for u8 {
    type Str = [u8];
    type Collection = Vec<u8>;

    fn is_whitespace(&self) -> bool {
        self.is_ascii_whitespace()
    }
    fn digit_zero() -> Self {
        b'0'
    }
    fn is_digit(&self, radix: u32) -> bool {
        (*self as char).is_digit(radix)
    }
    fn to_char(&self) -> char {
        *self as char
    }
}

impl Character for char {
    type Str = str;
    type Collection = String;

    fn is_whitespace(&self) -> bool {
        char::is_whitespace(*self)
    }
    fn digit_zero() -> Self {
        '0'
    }
    fn is_digit(&self, radix: u32) -> bool {
        char::is_digit(*self, radix)
    }
    fn to_char(&self) -> char {
        *self
    }
}

/// A trait containing text-specific functionality that extends the [`Parser`] trait.
pub trait TextParser<I: Character, O>: Parser<I, O> {
    /// Parse a pattern, allowing whitespace both before and after.
    fn padded(self) -> Padded<Self, I, O>
    where
        Self: Sized,
    {
        whitespace().ignore_then(self).then_ignore(whitespace())
    }
}

impl<I: Character, O, P: Parser<I, O>> TextParser<I, O> for P {}

/// A parser that accepts (and ignores) any number of whitespace characters.
pub fn whitespace<C: Character, E: Error<C>>() -> Padding<C, E> {
    custom(|stream: &mut StreamOf<C, E>| loop {
        let state = stream.save();
        if stream.next().2.map_or(true, |b| !b.is_whitespace()) {
            stream.revert(state);
            break (Vec::new(), Ok(((), None)));
        }
    })
}

/// A parser that accepts (and ignores) any newline characters or character sequences.
pub fn newline<E: Error<char>>() -> impl Parser<char, (), Error = E> + Copy + Clone {
    just('\r')
        .or_not()
        .ignore_then(just('\n'))
        .or(just('\x0B')) // Vertical tab
        .or(just('\x0C')) // Form feed
        .or(just('\x0D')) // Carriage return
        .or(just('\u{0085}')) // Next line
        .or(just('\u{2028}')) // Line separator
        .or(just('\u{2029}')) // Paragraph separator
        .ignored()
}

/// A parser that accepts one or more ASCII digits.
pub fn digits<C: Character, E: Error<C>>(
    radix: u32,
) -> impl Parser<C, C::Collection, Error = E> + Copy + Clone {
    filter(move |c: &C| c.is_digit(radix))
        .repeated()
        .at_least(1)
        .collect()
}

/// A parser that accepts a positive integer.
///
/// An integer is defined as a non-empty sequence of ASCII digits, where the first digit is non-zero or the sequence
/// has length one.
pub fn int<C: Character, E: Error<C>>(
    radix: u32,
) -> impl Parser<C, C::Collection, Error = E> + Copy + Clone {
    filter(move |c: &C| c.is_digit(radix) && c != &C::digit_zero())
        .map(Some)
        .chain::<C, Vec<_>, _>(filter(move |c: &C| c.is_digit(radix)).repeated())
        .collect()
        .or(just(C::digit_zero()).map(|c| core::iter::once(c).collect()))
}

/// A parser that accepts a C-style identifier.
///
/// An identifier is defined as an ASCII alphabetic character or an underscore followed by any number of alphanumeric
/// characters or underscores. The regex pattern for it is `[a-zA-Z_][a-zA-Z0-9_]*`.
pub fn ident<C: Character, E: Error<C>>() -> impl Parser<C, C::Collection, Error = E> + Copy + Clone
{
    filter(|c: &C| c.to_char().is_ascii_alphabetic() || c.to_char() == '_')
        .map(Some)
        .chain::<C, Vec<_>, _>(
            filter(|c: &C| c.to_char().is_ascii_alphanumeric() || c.to_char() == '_').repeated(),
        )
        .collect()
}

/// Like [`ident`], but only accepts an exact identifier while ignoring trailing identifier characters.
///
/// # Example
///
/// ```
/// # use chumsky::prelude::*;
/// let def = text::keyword::<_, _, Simple<char>>("def");
///
/// // Exactly 'def' was found
/// assert_eq!(def.parse("def"), Ok(()));
/// // Exactly 'def' was found, with non-identifier trailing characters
/// assert_eq!(def.parse("def(foo, bar)"), Ok(()));
/// // 'def' was found, but only as part of a larger identifier, so this fails to parse
/// assert!(def.parse("define").is_err());
/// ```
pub fn keyword<'a, C: Character + 'a, S: AsRef<C::Str> + 'a + Clone, E: Error<C> + 'a>(keyword: S) -> impl Parser<C, (), Error = E> + Clone + 'a {
    // TODO: use .filter(...), improve error messages
    ident().try_map(move |s: C::Collection, span| if s.as_ref() == keyword.as_ref() {
        Ok(())
    } else {
        Err(E::expected_input_found(span, None, None))
    })
}
