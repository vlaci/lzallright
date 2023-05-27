from array import array
from mmap import mmap
from typing import Optional, Tuple, Union

_BufferType = Union[array[int], bytes, bytearray, memoryview, mmap]

class LZOCompressor:
    """Object containing the compressor state.

    Thread safety:
        It is not allowed to pass instances of this class between threads.
    """

    def compress(self, data: _BufferType) -> bytes:
        """Compresses data.

        Subsequent invocations of this method reuses the compression state.  In other
        words, the total output size doesn't change if you call this method using one
        big buffer or multiple small ones.

        Args:
            data (bytes): Any python object that implements the buffer protocol

        Note:
            The GIL (Global Interpreter Lock) is released for the duration of this
            function.
        """
    @staticmethod
    def decompress(data: _BufferType, output_size_hint: Optional[int] = None) -> bytes:
        """Decompresses data.

        Args:
            data (bytes): Any python object that implements the buffer protocol
            output_size_hint: Preallocate output buffer to this size.
                Helps reducing memory overhead if decompressed size is known in advance.

        Note:
            The GIL (Global Interpreter Lock) is released for the duration of this
            function.
        """

class EResult:
    """Error codes in [`LZOError.args[0]`][lzallright._lzallright.LZOError.args]."""

    LookbehindOverrun = ...
    "Invalid input"
    OutputOverrun = ...
    """Buffer is not long enough to hold output.

    Warning:
        lzallright always tries to find the appropriate buffer size.
        If you see this, it is an error.
    """
    InputOverrun = ...
    "Input to decompress is truncated"
    Error = ...
    "Other error occurred"
    InputNotConsumed = ...
    "See [`InputNotConsumed`][lzallright._lzallright.InputNotConsumed]"

class LZOError(Exception):
    """Fatal error during compression/decompression."""

    args: Tuple[EResult]
    """Error reason.

    See [`EResult`][lzallright._lzallright.EResult]"""

class InputNotConsumed(LZOError):  # noqa: N818
    """Decompression finished with leftover data."""

    args: Tuple[EResult, bytes]
    """Error reason, with decompressed data

    ``(EResult.InputNotConsumed, decompressed: bytes)``
    """
