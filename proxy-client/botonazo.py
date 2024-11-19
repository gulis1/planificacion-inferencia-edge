import sys
from datetime import datetime, timedelta
from time import sleep
import subprocess

if len(sys.argv) < 3:
    print("Usage: botonazo.py <datetime> <command>")
    print("Date format: 2024-9-25-16:11:01")
    exit()

now = datetime.now()
target = datetime.strptime(sys.argv[1], "%Y-%m-%d-%H:%M:%S")
delay = (target - now).total_seconds()
sleep(delay)

output = subprocess.check_output(sys.argv[2:], stderr=subprocess.STDOUT, shell=True).decode("utf-8")

print(output, end="")



