"""Prepare the local MP4 selected for this browser Run."""

from _noon_errors import engine_await


class VideoSource:
    """Immutable source for Scene.video, prepared once before play or wait."""

    __slots__ = ("_handle",)

    def __init__(self):
        raise TypeError("use await VideoSource.from_upload()")

    @classmethod
    async def from_upload(cls):
        """Prepare the MP4 selected with the playground's Video file control."""
        try:
            from _noon_host import noonLoadUploadedVideo
        except ImportError as error:
            raise RuntimeError("native Scene.video accepts a local MP4 path") from error
        source = object.__new__(cls)
        source._handle = await engine_await(noonLoadUploadedVideo(), operation="VideoSource.from_upload")
        return source
