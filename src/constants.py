from enum import Enum
from pathlib import Path


class Config(Enum):
    """Configuration constants for avatar generation."""

    # File paths and directories
    DEFAULT_OUTPUT_DIR = "avatars"
    EXPIRY_TRACKING_FILE = Path(".avatar_expiry.pkl").absolute()

    # RoboHash API
    ROBOHASH_BASE_URL = "https://robohash.org/"
    # Available sets: set1=robots(default), set2=monsters, set3=robots(heads), set4=kittens
    ROBOHASH_SETS = ["set1", "set2", "set3", "set4"]
    ROBOHASH_DEFAULT_SET = "set1"

    # Avatar generation parameters
    BACKGROUND_COLOR = "white"
    TEXT_COLOR = "black"
    FONT_SIZE_RATIO = 0.5
    MARGIN_RATIO = 0.1
    AVATAR_EXTENSION = ".png"
