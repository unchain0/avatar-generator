from pathlib import Path

DEFAULT_OUTPUT_DIR = "avatars"
# Tracking file in the root directory, ensure it's absolute
EXPIRY_TRACKING_FILE = Path(".avatar_expiry.pkl").absolute()
