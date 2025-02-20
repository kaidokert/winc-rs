""" Wrapper script to run host test fixtures """
import sys
import subprocess
import time
import os

def is_test_binary(binary_path):
    return ('deps' in binary_path and '-' in os.path.basename(binary_path))

def main():
    args = sys.argv[1:]  # Skip the script name

    # The last argument will be the binary path
    binary_path = args[-1]
    is_test = is_test_binary(binary_path)

    http_server = None

    if is_test:
        print(f"Detected test binary, starting HTTP server...")
        # Universal approach: redirect output to devnull and run in background
        http_server = subprocess.Popen(
            ['python', '-m', 'http.server', '-b', '0.0.0.0', '8005'],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            # This ensures child process won't receive parent's signals
            start_new_session=True
        )
        print(f"HTTP server started with pid: {http_server.pid}")
        time.sleep(1)

    try:
        print(f"Running command: {' '.join(args)}")
        process = subprocess.run(args, check=False)
        return_code = process.returncode
    finally:
        if http_server:
            print("Cleaning up HTTP server...")
            try:
                # Send SIGTERM first for graceful shutdown
                http_server.terminate()
                http_server.wait(timeout=2)
            except subprocess.TimeoutExpired:
                # If that doesn't work, force kill
                http_server.kill()
                http_server.wait()
            print("HTTP server cleaned up")

    sys.exit(return_code)

if __name__ == '__main__':
    main()
