from enum import Enum
from pathlib import Path


class Config(Enum):
    """Configuration constants for avatar generation."""

    DEFAULT_OUTPUT_DIR = "avatars"
    EXPIRY_TRACKING_FILE = Path(".avatar_expiry.pkl").absolute()

    BACKGROUND_COLOR = "white"
    TEXT_COLOR = "black"
    FONT_SIZE_RATIO = 0.5
    MARGIN_RATIO = 0.1
    AVATAR_EXTENSION = ".png"
