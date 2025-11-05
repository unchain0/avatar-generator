import requests
import responses
import pytest
from unittest.mock import patch

from src.avatar import fetch_and_save_avatar

@pytest.fixture
def mock_responses():
    with responses.RequestsMock() as rsps:
        yield rsps

def test_fetch_and_save_avatar_success(tmp_path, mock_responses):
    """
    Tests successful fetching and saving of an avatar.
    """
    input_string = "test_string"
    url = f"https://robohash.org/{input_string}"
    mock_responses.add(responses.GET, url, body=b"avatar_image_data", status=200)

    with patch("src.avatar.add_expiry_tracking") as mock_add_expiry:
        saved_path = fetch_and_save_avatar(input_string, output_dir=str(tmp_path))
        assert saved_path is not None
        assert saved_path.exists()
        assert saved_path.read_bytes() == b"avatar_image_data"
        mock_add_expiry.assert_not_called()

def test_fetch_and_save_avatar_with_expiration(tmp_path, mock_responses):
    """
    Tests successful fetching and saving with expiration tracking.
    """
    input_string = "test_string_expiry"
    url = f"https://robohash.org/{input_string}"
    mock_responses.add(responses.GET, url, body=b"avatar_data", status=200)

    with patch("src.avatar.add_expiry_tracking") as mock_add_expiry:
        saved_path = fetch_and_save_avatar(
            input_string, output_dir=str(tmp_path), expiration_days=5
        )
        assert saved_path is not None
        assert saved_path.exists()
        mock_add_expiry.assert_called_once()

def test_fetch_and_save_avatar_empty_input():
    """
    Tests that the function returns None for an empty input string.
    """
    assert fetch_and_save_avatar("") is None

def test_fetch_and_save_avatar_request_exception(tmp_path, mock_responses):
    """
    Tests the function's handling of a requests exception.
    """
    input_string = "test_exception"
    url = f"https://robohash.org/{input_string}"
    mock_responses.add(
        responses.GET, url, body=requests.exceptions.RequestException("Connection error")
    )

    saved_path = fetch_and_save_avatar(input_string, output_dir=str(tmp_path))
    assert saved_path is None

def test_fetch_and_save_avatar_http_error(tmp_path, mock_responses):
    """
    Tests the function's handling of an HTTP error status.
    """
    input_string = "test_http_error"
    url = f"https://robohash.org/{input_string}"
    mock_responses.add(responses.GET, url, status=404)

    saved_path = fetch_and_save_avatar(input_string, output_dir=str(tmp_path))
    assert saved_path is None

@patch("pathlib.Path.open")
def test_fetch_and_save_avatar_io_error(mock_open, tmp_path, mock_responses):
    """
    Tests the function's handling of an IOError during file saving.
    """
    mock_open.side_effect = IOError("Permission denied")
    input_string = "test_io_error"
    url = f"https://robohash.org/{input_string}"
    mock_responses.add(responses.GET, url, body=b"image_data", status=200)

    saved_path = fetch_and_save_avatar(input_string, output_dir=str(tmp_path))
    assert saved_path is None
