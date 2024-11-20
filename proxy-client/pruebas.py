from argparse import ArgumentParser
import subprocess
from threading import Thread
from time import sleep
import json
import numpy as np
import pickle
from datetime import datetime
import sys
import numpy as np
import random

def argument_parser() -> ArgumentParser:

    parser = ArgumentParser()
    parser.add_argument(
        "-u",
        "--url",
        type=str,
        required=False,
        help="Inference server URL. Default is localhost:8000.",
    )

    parser.add_argument(
        "-i",
        "--image",
        type=str,
        required=False,
        help="Image path",
    )

    parser.add_argument(
        "-l",
        "--load",
        type=str,
        required=False,
        help="Load data from file",
    )

    parser.add_argument(
        "-t",
        "--time",
        type=int,
        required=False,
        help="Time between requests (ms)",
    )

    parser.add_argument(
        "-n",
        "--nreq",
        type=int,
        required=False,
        help="Number of requests",
    )

    parser.add_argument(
        "--gen-json",
        type=str,
        required=False,
        help="Generate pod JSON for the given namespace",
    )

    parser.add_argument(
        "-w",
        "--wait",
        type=str,
        required=False,
        help="Wait until given time to run the experiment (Format: 2024-9-25-16:11:01)",
    )

    return parser

def gen_pod_json(namespace):

    output = subprocess.check_output([
        "kubectl",
        f"--namespace={namespace}",
        "get",
        "pods",
        "-o",
        "custom-columns=NODE:.spec.nodeName,PodUID:.metadata.uid,status:status.phase"
    ]).decode("utf8")

    lines = [ line.split() for line in output.splitlines() ]
    pods = { line[1]:line[0] for line in lines if line[-1] == "Running" }
    
    with open("pods.json", "w") as file:
        json.dump(pods, file)
    
    return pods

def launch_client(args, thread_id: int, output_vec: list[str]):
    
    prioridad = 10000
    accuracy = 60 
    
    launch_args = [
        "python3",
        "client.py",
        "-u",
        args.url,
        "-i",
        args.image,
        "-p",
        str(prioridad),
        "-a",
        str(accuracy)
    ]
    
    if random.random() > 0.5:
        launch_args.extend(["-q", "int8"])
    else:
        launch_args.extend(["-q", "tf32"])
    output = subprocess.check_output(launch_args).decode("utf8")
    print("Hola")
    output += "\nPrioridad: " + str(prioridad)
    output += "\nAccuracy: " + str(accuracy)
    print(f"Thread {thread_id} finished") 
    print(output)
    output_vec[thread_id] = output

class InferenceResult:
    total_ms: int
    model: str
    route: str
    prioridad: int
    accuracy: int
    t_inferencia: int
    t_pprocesado: int

    def __init__(self):
        self.total_ms = None
        self.model = None
        self.route = None
        self.prioridad = None
        self.accuracy = None
        self.t_inferencia = None
        self.t_pprocesado = None
    
def parse_result(c: str, pods) -> InferenceResult:
    lines = [line.split(":") for line in c.splitlines() if len(line) > 0]
    
    result = InferenceResult()
    for line in lines:
        left = line[0]
        right = line[1].strip()
        
        if left == "Took":
            result.total_ms = round(float(right[:-3]), 2)
        elif left == "Model":
            result.model = right
        elif left == "Route":
            for uuid, name in pods.items():
                right = right.replace(uuid, name)
            result.route = right
        elif left == "Prioridad":
            result.prioridad = int(right)
        elif left == "Accuracy":
            result.accuracy = int(right)
        elif left == "Tiempo inferencia (ms)":
            result.t_inferencia = round(float(right), 2)
        elif left == "Tiempo preprocesado (ms)":
            result.t_pprocesado = round(float(right), 2)

    return result

def pruebas(args):

    pods = None
    
    results = []
    if args.load:
        with open(args.load) as file:
            content = json.load(file)
            args.time = content["time"]
            args.nreq = content["nreq"]
            for row in content["results"]:
                res = InferenceResult()
                res.__dict__ = row
                results.append(res)

    else:
        with open("pods.json") as file:
            pods = json.load(file)

        output_vec = [None for _ in range(args.nreq)]
        threads = []
        if args.wait:
            now = datetime.now()
            target = datetime.strptime(args.wait, "%Y-%m-%d-%H:%M:%S")
            delay = (target - now).total_seconds()
            sleep(delay)

        for i in range(args.nreq):
            t = Thread(target = lambda: launch_client(args, i, output_vec))
            t.start()
            threads.append(t)
            sleep(float(args.time) / 1000.0)
        
        for t in threads:
            t.join()
        
        results = [parse_result(o, pods) for o in output_vec]
        with open("datos.json", "w") as file:
            content = {
                "time": args.time,
                "nreq": args.nreq,
                "results": [res.__dict__ for res in results]
            }
            json.dump(content, file, indent=4)
    
def main():

    random.seed()
    
    parser = argument_parser()
    args = parser.parse_args()

    if args.gen_json:
        gen_pod_json(args.gen_json)
    else:
        pruebas(args)

if __name__ == "__main__":
    main()

