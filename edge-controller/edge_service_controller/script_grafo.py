#!/usr/bin/python3

import subprocess
from sys import argv
import json

def get_pods(namespace):

    service_uuid = subprocess.check_output(f"sudo kubectl --namespace={namespace} describe edgeservice | grep UID", shell=True).decode("utf8").split()[1]
    output = subprocess.check_output([
        "sudo",
        "kubectl",
        f"--namespace={namespace}",
        "get",
        "pods",
        "-o",
        "custom-columns=NODE:.spec.nodeName,PodUID:.metadata.uid,status:status.phase"
    ]).decode("utf8")

    lines = [ line.split() for line in output.splitlines() ]
    pods = { line[0]:line[1] for line in lines if line[-1] == "Running" }

    return service_uuid, pods

def main():

    if len(argv) != 4:
        print("Usage: script_grafo.py <namespace> <input_file> <output file>")
        exit(1)

    namespace = argv[1]
    input_file = argv[2]
    output_file = argv[3]
    service_uuid, pods = get_pods(namespace)
    with open(input_file) as input_file:
        content = input_file.read()
        for pod_name, pod_uuid in pods.items():
            content = content.replace(pod_name, f'"{pod_uuid}"')
        
        print(content)
        with open(output_file, "w") as output_file:
            output = {
                "service_uuid": service_uuid,
                "graph": content
            }
            output = json.dumps(output, indent=4)
            output_file.write(output)

main()
