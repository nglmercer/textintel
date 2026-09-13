class TextIntelError(Exception):
    """Base error for the library."""


class InputTooLongError(TextIntelError):
    """Input exceeds configured max_input_length."""
