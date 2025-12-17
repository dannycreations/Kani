import os
import sys
import subprocess

try:
    import xlsxwriter
except ImportError:
    print("xlsxwriter not found. Please install it using: pip install xlsxwriter")
    sys.exit(1)


def create_plain(filename):
    workbook = xlsxwriter.Workbook(filename)
    worksheet = workbook.add_worksheet()
    worksheet.write("A1", "Hello")
    workbook.close()
    print(f"Created {filename}")


def create_protected(filename):
    workbook = xlsxwriter.Workbook(filename)
    worksheet = workbook.add_worksheet("MyProtectedSheet")
    worksheet.write("A1", "Locked")
    worksheet.protect("pass1")
    worksheet = workbook.add_worksheet("MyUnprotectedSheet")
    worksheet.write("A1", "Open")
    worksheet = workbook.add_worksheet("AnotherProtected")
    worksheet.write("A1", "LockedToo")
    worksheet.protect("pass2")
    workbook.close()
    print(f"Created {filename}")


def create_encrypted(filename, password="password"):
    # Create a plain file first
    temp_filename = filename + ".temp.xlsx"
    workbook = xlsxwriter.Workbook(temp_filename)
    worksheet = workbook.add_worksheet()
    worksheet.write("A1", "Secret Data")
    workbook.close()

    # Encrypt using msoffcrypto-tool CLI
    try:
        # Check if msoffcrypto-tool is available in path
        subprocess.run(
            ["msoffcrypto-tool", "--help"],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        # Encrypt
        cmd = ["msoffcrypto-tool", temp_filename, filename, "-p", password, "-e"]
        subprocess.run(cmd, check=True)
        print(f"Created {filename} (using msoffcrypto-tool)")

        # Verify OLE
        with open(filename, "rb") as f:
            header = f.read(8)
            if header == b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1":
                print(f"Verified {filename} is OLE Encrypted")
            else:
                print(
                    f"Warning: {filename} is NOT OLE Encrypted. Header: {header.hex()}"
                )

    except (subprocess.CalledProcessError, FileNotFoundError):
        print(
            "msoffcrypto-tool failed or not found. Using fallback (xlsxwriter constructor)."
        )
        # Fallback
        workbook = xlsxwriter.Workbook(filename, {"password": password})
        worksheet = workbook.add_worksheet()
        worksheet.write("A1", "Secret Data")
        workbook.close()
        print(f"Created {filename} (fallback)")
    finally:
        if os.path.exists(temp_filename):
            os.remove(temp_filename)


def main():
    output_dir = "crates/excel_unprotect/tests/fixtures"
    if not os.path.exists(output_dir):
        os.makedirs(output_dir)

    create_plain(os.path.join(output_dir, "plain.xlsx"))
    create_protected(os.path.join(output_dir, "protected.xlsx"))
    create_encrypted(os.path.join(output_dir, "encrypted.xlsx"))

    # Corrupted / Non-zip file
    with open(os.path.join(output_dir, "corrupt.xlsx"), "w") as f:
        f.write("This is not a zip file")
    print(f"Created corrupted file")


if __name__ == "__main__":
    main()
