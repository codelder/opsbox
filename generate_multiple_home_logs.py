#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Generate multiple ``home`` log archives across day ranges."""

import datetime
import os
import shutil
from pathlib import Path

from log_generation_common import create_directory_structure, create_tar_gz, generate_files


def main() -> None:
    print("Starting batched log archive generation ...")

    for batch in [20, 21, 22, 23]:
        print(f"\n{'=' * 50}")
        print(f"Batch {batch} generation started")
        print(f"{'=' * 50}")

        end_date = datetime.date(2025, 11, 6)
        start_date = end_date - datetime.timedelta(days=10)

        for i in range(11):
            current_date = start_date + datetime.timedelta(days=i)
            date_str = current_date.strftime("%Y-%m-%d")

            print(f"\n=== Batch {batch}, file {i + 1}, date: {date_str} ===")

            base_dir = Path(f"home_batch{batch}_{i + 1}")
            create_directory_structure(base_dir, date_str)
            print(f"Created directory structure at: {base_dir}")

            generate_files(base_dir, date_str)

            output_file = Path(f"BBIP_{batch}_APPLOG_{date_str}.tar.gz")
            create_tar_gz(base_dir, output_file)

            shutil.rmtree(base_dir)

            size_kb = os.path.getsize(output_file) / 1024
            print(f"Finished archive: {output_file}")
            print(f"Archive size: {size_kb:.1f} KB")

        print(f"\nBatch {batch} completed!")

    print(f"\n{'=' * 50}")
    print("All batches complete!")
    print(f"{'=' * 50}")


if __name__ == "__main__":
    main()
