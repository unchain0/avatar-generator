import argparse
import hashlib
import uuid
from pathlib import Path

from tqdm import tqdm

from .avatar import fetch_and_save_avatar
from .constants import DEFAULT_OUTPUT_DIR, EXPIRY_TRACKING_FILE
from .expiry import cleanup_all_avatars, cleanup_expired_avatars


def main():
    """Parses command line arguments, cleans up expired avatars, and generates new ones.

    Sets up argument parsing for the number of avatars, output directory, and
    expiration duration. Calls cleanup_expired_avatars first. Then, generates the
    requested number of avatars using fetch_and_save_avatar, displaying a progress
    bar via tqdm. Finally, prints a summary of generated and failed counts.
    """
    parser = argparse.ArgumentParser(description="Generate RoboHash avatars.")
    parser.add_argument(
        "-n",
        "--number",
        type=int,
        default=1,
        help="Number of avatars to generate (default: 1)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=str,
        default=DEFAULT_OUTPUT_DIR,
        help=f"Output directory for avatars (default: '{DEFAULT_OUTPUT_DIR}')",
    )
    parser.add_argument(
        "--expires-in",
        type=int,
        default=1,
        metavar="DAYS",
        help="Delete avatar automatically after this many days (requires running the script again). Default is 1 day.",
    )
    parser.add_argument(
        "--cleanup-all",
        action="store_true",
        help="Delete ALL existing avatars in the output directory before generating new ones. Overrides --expires-in for cleanup phase.",
    )
    args = parser.parse_args()

    # --- Determine paths --- Helper for clarity
    output_path = Path(args.output).absolute()
    # EXPIRY_TRACKING_FILE is already absolute from constants.py

    # --- Cleanup Phase --- Decide which cleanup to run
    if args.cleanup_all:
        cleanup_all_avatars(output_path, EXPIRY_TRACKING_FILE)
    else:
        cleanup_expired_avatars(EXPIRY_TRACKING_FILE)
    print("-" * 50)

    # --- Generate new avatars ---
    if args.number < 1:
        print("Error: Number of avatars must be at least 1.")
        return

    print(f"Generating {args.number} avatar(s) in directory '{args.output}'...")
    generated_count = 0
    failed_count = 0
    with tqdm(total=args.number, unit="avatar", desc="Generating Avatars") as pbar:
        for _ in range(args.number):
            # Generate a unique hash for each avatar request
            random_data = uuid.uuid4().hex.encode("utf-8")
            sha256_hash = hashlib.sha256(random_data).hexdigest()

            saved_path = fetch_and_save_avatar(
                sha256_hash,
                output_dir=args.output,
                expiration_days=args.expires_in,
                tracking_file=EXPIRY_TRACKING_FILE,
            )
            if saved_path:
                generated_count += 1
            else:
                failed_count += 1
            pbar.update(1)

    print("-" * 50)
    print(
        f"Avatar generation complete. Generated: {generated_count}, Failed: {failed_count}"
    )


# Script entry point
if __name__ == "__main__":
    main()
