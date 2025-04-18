# Avatar Generator

A simple Python script to generate unique robot avatars using the [RoboHash API](https://robohash.org/) and save them locally. It includes features for generating multiple avatars and setting an expiration time for automatic cleanup.

## Features

- Generate unique avatars based on random strings.
- Specify the number of avatars to generate.
- Specify a custom output directory.
- Set an expiration duration (in days) for generated avatars.
- Automatic cleanup of expired avatars upon running the script.

## Installation

1. **Clone the repository:**

    ```bash
    git clone https://github.com/unchain0/avatar-generator
    cd avatar-generator
    ```

2. **Install dependencies:**
    This project uses Poetry for dependency management.

    ```bash
    poetry install
    ```

    Alternatively, if you have the dependencies (`requests`, `tqdm`) installed globally or in a virtual environment, you might be able to run it directly.

## Usage

Run the script from the root directory using Python:

```bash
python main.py [OPTIONS]
```

**Options:**

- `-n NUMBER`, `--number NUMBER`: Number of avatars to generate (default: 1).
- `-o DIRECTORY`, `--output DIRECTORY`: Output directory for avatars (default: `avatars`).
- `--expires-in DAYS`: Set an expiration time in days for the generated avatars. Expired avatars are deleted the next time the script runs based on this value (default: 1 day). This is ignored if `--cleanup-all` is used for the cleanup phase.
- `--cleanup-all`: If present, delete ALL existing avatars in the output directory and clear the tracking file *before* generating new ones.

**Examples:**

- Generate 5 avatars in the default `avatars` directory, expiring in 7 days:

    ```bash
    python main.py -n 5 --expires-in 7
    ```

- Generate 1 avatar in a custom directory `my_robots`, with default expiration (1 day):

    ```bash
    python main.py -o my_robots
    ```

- Generate 10 avatars, but first delete all existing avatars in the `avatars` directory:

    ```bash
    python main.py -n 10 --cleanup-all
    ```

## How Expiration Works

When you use the `--expires-in` option, the script records the file path and its expiration date in a `.avatar_expiry.pkl` file in the project root. Each time the script runs, it first checks this file and deletes any avatars whose expiration date has passed.
