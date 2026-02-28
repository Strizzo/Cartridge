"""
Safe mathematical expression parser and evaluator.
Supports: +, -, *, /, %, parentheses, decimal numbers, negation.
Does NOT use eval() -- uses a recursive descent parser.
"""

import math
from typing import Optional


class ParseError(Exception):
    """Raised when the expression cannot be parsed."""
    pass


class DivisionByZeroError(Exception):
    """Raised on division by zero."""
    pass


class Token:
    NUMBER = "NUMBER"
    PLUS = "PLUS"
    MINUS = "MINUS"
    MUL = "MUL"
    DIV = "DIV"
    PERCENT = "PERCENT"
    LPAREN = "LPAREN"
    RPAREN = "RPAREN"
    EOF = "EOF"

    def __init__(self, kind: str, value: object = None):
        self.kind = kind
        self.value = value

    def __repr__(self):
        return f"Token({self.kind}, {self.value!r})"


class Lexer:
    """Tokenizes a mathematical expression string."""

    def __init__(self, text: str):
        self.text = text
        self.pos = 0

    def _skip_whitespace(self):
        while self.pos < len(self.text) and self.text[self.pos] in " \t":
            self.pos += 1

    def _read_number(self) -> Token:
        start = self.pos
        has_dot = False
        while self.pos < len(self.text):
            ch = self.text[self.pos]
            if ch == ".":
                if has_dot:
                    break
                has_dot = True
                self.pos += 1
            elif ch.isdigit():
                self.pos += 1
            else:
                break
        text = self.text[start : self.pos]
        if text == ".":
            return Token(Token.NUMBER, 0.0)
        return Token(Token.NUMBER, float(text))

    def next_token(self) -> Token:
        self._skip_whitespace()
        if self.pos >= len(self.text):
            return Token(Token.EOF)

        ch = self.text[self.pos]

        if ch.isdigit() or ch == ".":
            return self._read_number()

        self.pos += 1
        if ch == "+":
            return Token(Token.PLUS)
        elif ch == "-":
            return Token(Token.MINUS)
        elif ch == "*":
            return Token(Token.MUL)
        elif ch == "/":
            return Token(Token.DIV)
        elif ch == "%":
            return Token(Token.PERCENT)
        elif ch == "(":
            return Token(Token.LPAREN)
        elif ch == ")":
            return Token(Token.RPAREN)
        else:
            raise ParseError(f"Unexpected character: {ch!r}")

    def tokenize(self) -> list:
        tokens = []
        while True:
            tok = self.next_token()
            tokens.append(tok)
            if tok.kind == Token.EOF:
                break
        return tokens


class Parser:
    """
    Recursive descent parser for mathematical expressions.

    Grammar:
        expr       -> term (('+' | '-') term)*
        term       -> unary (('*' | '/') unary)*
        unary      -> ('-')* postfix
        postfix    -> primary ('%')*
        primary    -> NUMBER | '(' expr ')'
    """

    def __init__(self, tokens: list):
        self.tokens = tokens
        self.pos = 0

    def _current(self) -> Token:
        if self.pos < len(self.tokens):
            return self.tokens[self.pos]
        return Token(Token.EOF)

    def _consume(self, kind: str) -> Token:
        tok = self._current()
        if tok.kind != kind:
            raise ParseError(f"Expected {kind}, got {tok.kind}")
        self.pos += 1
        return tok

    def _match(self, *kinds) -> Optional[Token]:
        tok = self._current()
        if tok.kind in kinds:
            self.pos += 1
            return tok
        return None

    def parse(self) -> float:
        result = self._expr()
        if self._current().kind != Token.EOF:
            raise ParseError("Unexpected token after expression")
        return result

    def _expr(self) -> float:
        left = self._term()
        while True:
            tok = self._match(Token.PLUS, Token.MINUS)
            if tok is None:
                break
            right = self._term()
            if tok.kind == Token.PLUS:
                left = left + right
            else:
                left = left - right
        return left

    def _term(self) -> float:
        left = self._unary()
        while True:
            tok = self._match(Token.MUL, Token.DIV)
            if tok is None:
                break
            right = self._unary()
            if tok.kind == Token.MUL:
                left = left * right
            else:
                if right == 0:
                    raise DivisionByZeroError("Division by zero")
                left = left / right
        return left

    def _unary(self) -> float:
        if self._match(Token.MINUS):
            return -self._unary()
        return self._postfix()

    def _postfix(self) -> float:
        value = self._primary()
        while self._match(Token.PERCENT):
            value = value / 100.0
        return value

    def _primary(self) -> float:
        tok = self._current()
        if tok.kind == Token.NUMBER:
            self.pos += 1
            return tok.value
        if tok.kind == Token.LPAREN:
            self.pos += 1
            value = self._expr()
            self._consume(Token.RPAREN)
            return value
        raise ParseError(f"Unexpected token: {tok.kind}")


def evaluate(expression: str) -> float:
    """
    Safely evaluate a mathematical expression string.
    Returns the result as a float.
    Raises ParseError or DivisionByZeroError on failure.
    """
    expression = expression.strip()
    if not expression:
        raise ParseError("Empty expression")

    lexer = Lexer(expression)
    tokens = lexer.tokenize()
    parser = Parser(tokens)
    return parser.parse()


def format_result(value: float) -> str:
    """
    Format a numeric result for display.
    - Integers show without decimal point
    - Floats show up to 10 significant digits, strip trailing zeros
    - Very large/small numbers use scientific notation
    """
    if math.isinf(value):
        return "Infinity" if value > 0 else "-Infinity"
    if math.isnan(value):
        return "NaN"

    # Check if it's effectively an integer
    if value == int(value) and abs(value) < 1e15:
        return str(int(value))

    # For very large or very small numbers, use scientific notation
    if abs(value) >= 1e15 or (abs(value) < 1e-6 and value != 0):
        return f"{value:.6g}"

    # Normal float formatting
    formatted = f"{value:.10g}"
    return formatted


def try_evaluate(expression: str) -> Optional[str]:
    """
    Attempt to evaluate an expression.
    Returns the formatted result string, or None if invalid.
    """
    try:
        result = evaluate(expression)
        return format_result(result)
    except (ParseError, DivisionByZeroError, ZeroDivisionError):
        return None
    except Exception:
        return None
