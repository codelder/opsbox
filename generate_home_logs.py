#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Generate a single ``home`` log archive for local testing."""

import os
import shutil
from pathlib import Path

from log_generation_common import (
    DEFAULT_TRACE_DATE,
    create_directory_structure,
    create_tar_gz,
    generate_files,
)


def main() -> None:
    print("Starting log archive generation ...")

    base_dir = create_directory_structure(Path("home"), DEFAULT_TRACE_DATE)
    print(f"Created directory structure at: {base_dir}")

    generate_files(base_dir, DEFAULT_TRACE_DATE)

    output_file = Path("home_logs.tar.gz")
    create_tar_gz(base_dir, output_file)

    shutil.rmtree(base_dir)

    size_kb = os.path.getsize(output_file) / 1024
    print(f"Done! Archive written to: {output_file}")
    print(f"Archive size: {size_kb:.1f} KB")


if __name__ == "__main__":
    main()
