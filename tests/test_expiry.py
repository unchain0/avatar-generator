import pickle
import pytest
from pathlib import Path
from datetime import datetime, timedelta, timezone
from unittest.mock import patch

from src.expiry import (
    load_expiry_data,
    save_expiry_data,
    cleanup_expired_avatars,
    add_expiry_tracking,
    cleanup_all_avatars,
)

@pytest.fixture
def temp_tracking_file(tmp_path):
    """Provides a temporary tracking file for tests."""
    return tmp_path / "expiry.pkl"

def test_load_expiry_data_no_file(temp_tracking_file):
    """Tests that loading from a non-existent file returns an empty list."""
    assert load_expiry_data(temp_tracking_file) == []

def test_load_expiry_data_invalid_pickle(temp_tracking_file):
    """Tests that loading from a corrupted pickle file returns an empty list."""
    temp_tracking_file.write_text("invalid data")
    assert load_expiry_data(temp_tracking_file) == []

def test_save_and_load_expiry_data(temp_tracking_file):
    """Tests that data can be saved and loaded correctly."""
    now = datetime.now(timezone.utc)
    data = [{"filepath": "/path/to/avatar.png", "expires_at": now}]
    save_expiry_data(data, temp_tracking_file)
    loaded_data = load_expiry_data(temp_tracking_file)
    assert loaded_data == data

def test_add_expiry_tracking(temp_tracking_file):
    """Tests adding a new entry to the tracking file."""
    avatar_path = Path("/fake/avatar.png").absolute()
    add_expiry_tracking(avatar_path, 5, temp_tracking_file)
    data = load_expiry_data(temp_tracking_file)
    assert len(data) == 1
    assert data[0]["filepath"] == str(avatar_path)

def test_cleanup_expired_avatars(temp_tracking_file, tmp_path):
    """Tests cleanup of expired avatars."""
    now = datetime.now(timezone.utc)
    expired_avatar = tmp_path / "expired.png"
    expired_avatar.touch()

    active_avatar = tmp_path / "active.png"
    active_avatar.touch()

    data = [
        {"filepath": str(expired_avatar), "expires_at": now - timedelta(days=1)},
        {"filepath": str(active_avatar), "expires_at": now + timedelta(days=1)},
    ]
    save_expiry_data(data, temp_tracking_file)

    cleanup_expired_avatars(temp_tracking_file)

    assert not expired_avatar.exists()
    assert active_avatar.exists()

    remaining_data = load_expiry_data(temp_tracking_file)
    assert len(remaining_data) == 1
    assert remaining_data[0]["filepath"] == str(active_avatar)

def test_cleanup_all_avatars(temp_tracking_file, tmp_path):
    """Tests the complete cleanup of all avatars."""
    avatar1 = tmp_path / "avatar1.png"
    avatar1.touch()
    avatar2 = tmp_path / "avatar2.png"
    avatar2.touch()

    data = [
        {"filepath": str(avatar1), "expires_at": datetime.now(timezone.utc)},
        {"filepath": str(avatar2), "expires_at": datetime.now(timezone.utc)},
    ]
    save_expiry_data(data, temp_tracking_file)

    cleanup_all_avatars(tmp_path, temp_tracking_file)

    assert not avatar1.exists()
    assert not avatar2.exists()

    remaining_data = load_expiry_data(temp_tracking_file)
    assert len(remaining_data) == 0

def test_load_expiry_data_invalid_format(temp_tracking_file, capsys):
    """Tests loading data with an invalid format."""
    with temp_tracking_file.open("wb") as f:
        pickle.dump({"invalid": "data"}, f)

    result = load_expiry_data(temp_tracking_file)
    assert result == []

    captured = capsys.readouterr()
    assert "Warning: Invalid data format" in captured.out

@patch("pickle.dump")
def test_save_expiry_data_pickle_error(mock_pickle_dump, temp_tracking_file, capsys):
    """Tests that a pickling error is handled correctly."""
    mock_pickle_dump.side_effect = pickle.PicklingError("Test error")

    save_expiry_data([{"key": "value"}], temp_tracking_file)

    captured = capsys.readouterr()
    assert "Warning: Could not write expiry tracking file" in captured.out

def test_add_expiry_tracking_no_expiration(temp_tracking_file):
    """Tests that no tracking is added if expiration_days is 0 or less."""
    avatar_path = Path("/fake/avatar.png")
    add_expiry_tracking(avatar_path, 0, temp_tracking_file)

    data = load_expiry_data(temp_tracking_file)
    assert len(data) == 0
