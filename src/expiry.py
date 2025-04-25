import pickle
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Dict, List

from .constants import Config


def load_expiry_data(
    tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value,
) -> List[Dict[str, Any]]:
    """Loads expiry tracking data from the pickle file.

    Args:
        tracking_file: Path to the pickle file containing expiry data.
            Defaults to EXPIRY_TRACKING_FILE.

    Returns:
        A list of dictionaries, where each dictionary represents an avatar's
        expiry information (e.g., {"filepath": str, "expires_at": datetime}).
        Returns an empty list if the file doesn't exist or is invalid.
    """
    if not tracking_file.is_file():
        return []
    try:
        with tracking_file.open("rb") as f:
            data = pickle.load(f)
            if isinstance(data, list):
                return data
            else:
                print(
                    f"Warning: Invalid data format in {tracking_file}. Expected list."
                )
                # Attempting to proceed might cause issues; returning empty list.
                return []
    except (pickle.UnpicklingError, IOError, EOFError) as e:
        print(f"Warning: Could not read expiry tracking file {tracking_file}: {e}")
        return []


def save_expiry_data(
    data: List[Dict[str, Any]], tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value
) -> None:
    """Saves expiry tracking data to the pickle file.

    Args:
        data: The list of expiry data dictionaries to save.
        tracking_file: Path to the pickle file where data will be saved.
            Defaults to EXPIRY_TRACKING_FILE.
    """
    try:
        with tracking_file.open("wb") as f:
            pickle.dump(data, f)
    except (pickle.PicklingError, IOError) as e:
        print(f"Warning: Could not write expiry tracking file {tracking_file}: {e}")


def cleanup_expired_avatars(
    tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value,
) -> None:
    """Checks the tracking file and deletes expired avatars.

    Reads the expiry data, iterates through entries, checks if the current time
    is past the 'expires_at' timestamp, and attempts to delete the corresponding
    file if it is expired. Updates the tracking file with remaining entries.

    Args:
        tracking_file: Path to the pickle file containing expiry data.
            Defaults to EXPIRY_TRACKING_FILE.
    """
    print("Checking for expired avatars...")
    expiry_data = load_expiry_data(tracking_file)
    if not expiry_data:
        print("No expiry data found or file is invalid.")
        return

    now = datetime.now(timezone.utc)
    updated_data = []
    deleted_count = 0

    for entry in expiry_data:
        try:
            filepath = Path(entry["filepath"])
            expires_at = entry["expires_at"]

            # Ensure expires_at is a timezone-aware datetime object
            if not isinstance(expires_at, datetime):
                try:
                    expires_at = datetime.fromisoformat(str(expires_at))
                except ValueError:
                    print(f"  Skipping invalid expires_at format: {entry}")
                    continue

            if expires_at.tzinfo is None:
                # Assume UTC if timezone info was missing when object was pickled
                expires_at = expires_at.replace(tzinfo=timezone.utc)

            if now > expires_at:
                if filepath.is_file():
                    try:
                        filepath.unlink()
                        print(f"  Deleted expired avatar: {filepath}")
                        deleted_count += 1
                    except OSError as e:
                        print(f"  Error deleting file {filepath}: {e}")
                        updated_data.append(entry)  # Keep entry if deletion failed
                else:
                    print(f"  Expired avatar not found or not a file: {filepath}")
            else:
                # Keep non-expired entries
                updated_data.append(entry)
        except (KeyError, TypeError) as e:
            print(f"  Skipping invalid/incomplete expiry entry: {entry} ({e})")

    if updated_data != expiry_data:
        save_expiry_data(updated_data, tracking_file)

    if deleted_count == 0:
        print("No expired avatars found to delete.")
    else:
        print(f"Cleanup complete. Deleted {deleted_count} expired avatar(s).")


def add_expiry_tracking(
    filepath: Path,
    expiration_days: int,
    tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value,
) -> None:
    """Adds expiry information for a newly created avatar to the pickle file.

    Calculates the expiration datetime based on the current time and
    expiration_days. Loads existing data, appends the new entry, and saves
    it back to the tracking file.

    Args:
        filepath: The absolute path to the newly created avatar file.
        expiration_days: The number of days from now until the avatar expires.
        tracking_file: Path to the pickle file for storing expiry data.
            Defaults to EXPIRY_TRACKING_FILE.
    """
    if expiration_days <= 0:
        return

    now = datetime.now(timezone.utc)
    expires_at = now + timedelta(days=expiration_days)
    expiry_data = load_expiry_data(tracking_file)

    # Store absolute path and timezone-aware datetime object
    expiry_data.append({"filepath": str(filepath.absolute()), "expires_at": expires_at})
    save_expiry_data(expiry_data, tracking_file)
    # Keep print minimal or remove if progress bar handles feedback
    # print(
    #    f"  Set expiration for {filepath.name} in {expiration_days} days ({expires_at.strftime('%Y-%m-%d %H:%M:%S %Z')})"
    # )


def cleanup_all_avatars(
    output_dir_path: Path, tracking_file: Path = Config.EXPIRY_TRACKING_FILE.value
) -> None:
    """Deletes all .png files in the specified output directory and clears the tracking file.

    Args:
        output_dir_path: The Path object representing the directory containing avatars.
        tracking_file: Path to the pickle file containing expiry data.
            Defaults to EXPIRY_TRACKING_FILE.
    """
    print(f"Attempting to delete all avatars in '{output_dir_path}'...")
    deleted_count = 0
    skipped_count = 0

    if not output_dir_path.is_dir():
        print(f"  Directory not found: {output_dir_path}. No avatars to delete.")
        # Still clear the tracking file in case it contains stale entries
        save_expiry_data([], tracking_file)
        print(f"  Cleared tracking file: {tracking_file}")
        return

    for item in output_dir_path.iterdir():
        if item.is_file() and item.suffix.lower() == Config.AVATAR_EXTENSION.value:
            try:
                item.unlink()
                print(f"  Deleted avatar: {item.name}")
                deleted_count += 1
            except OSError as e:
                print(f"  Error deleting file {item.name}: {e}")
                skipped_count += 1
        # else: # Optional: Log non-png files if needed
        #     print(f"  Skipping non-PNG file: {item.name}")

    # Clear the tracking file after deleting avatars
    save_expiry_data([], tracking_file)
    print(f"  Cleared tracking file: {tracking_file}")

    print(
        f"Cleanup complete. Deleted: {deleted_count}, Skipped/Errors: {skipped_count}."
    )
