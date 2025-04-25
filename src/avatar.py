from datetime import datetime
from pathlib import Path

import requests

from .constants import Config
from .expiry import add_expiry_tracking


def fetch_and_save_avatar(
    input_string: str,
    output_dir: str = Config.DEFAULT_OUTPUT_DIR.value,
    expiration_days: int | None = None,
    tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value,
) -> Path | None:
    """Fetches an avatar from RoboHash, saves it locally, and tracks expiration.

    Generates a filename based on the current timestamp. Creates the output
    directory if it doesn't exist. Fetches the image from the RoboHash API
    based on the input_string. If expiration_days is provided, it calls
    add_expiry_tracking to record the file for future cleanup.

    Args:
        input_string: The string used to generate the avatar via RoboHash API.
        output_dir: The directory where the avatar image will be saved.
            Defaults to DEFAULT_OUTPUT_DIR.
        expiration_days: Optional number of days until the avatar should expire.
            If provided and > 0, the file path and expiry date are recorded.
            Defaults to None (no expiration tracking).
        tracking_file: The Path object pointing to the expiration tracking file.
            Defaults to EXPIRY_TRACKING_FILE.

    Returns:
        The absolute Path to the saved avatar image on success, or None if
        fetching or saving failed.
    """
    if not input_string:
        print("Error: Input string cannot be empty.")
        return None

    url = f"https://robohash.org/{input_string}"

    # Generate filename based on current date and time
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
    filename = f"{timestamp}{Config.AVATAR_EXTENSION.value}"
    output_path = Path(output_dir).absolute()
    filepath = output_path / filename

    try:
        response = requests.get(url, stream=True)
        response.raise_for_status()  # Raise HTTPError for bad responses (4xx or 5xx)

        # Ensure output directory exists
        output_path.mkdir(parents=True, exist_ok=True)

        with filepath.open("wb") as f:
            for chunk in response.iter_content(chunk_size=8192):
                f.write(chunk)

        # Add tracking information if expiration is set
        if expiration_days is not None:
            add_expiry_tracking(filepath, expiration_days, tracking_file)
        return filepath  # Return the path on success

    except requests.exceptions.RequestException as e:
        # Consider adding more specific logging or error handling
        print(f"Error fetching avatar for {input_string}: {e}")
    except IOError as e:
        print(f"Error saving avatar {filepath}: {e}")

    return None  # Return None on failure
