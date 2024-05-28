from argparse import ArgumentParser
import subprocess
from threading import Thread
from time import sleep
import json
import matplotlib.pyplot as plt
import numpy as np

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

    #parser.add_argument(
    #    "-a",
    #    "--accuracy",
    #    type=int,
    #    required=False,
    #    default=0,
    #    help="Accuracy",
    #)

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
    
    prioridad = 1
    accuracy = 0

    output = subprocess.check_output([
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
    ]).decode("utf8")

    output += "\nPrioridad: " + str(prioridad)
    output += "\nAccuracy: " + str(accuracy)

    print(f"Thread {thread_id} finished") 
    output_vec[thread_id] = output

class InferenceResult:
    total_ms: int
    model: str
    route: str
    prioridad: int
    accuracy: int

    def __init__(self):
        self.total_ms = None
        self.model = None
        self.route = None
        self.prioridad = None
        self.accuracy = None
        pass
    
def parse_result(c: str, pods) -> InferenceResult:
    lines = [line.split(":") for line in c.splitlines() if len(line) > 0]
    
    result = InferenceResult()
    for line in lines:
        left = line[0]
        right = line[1].strip()
        
        if left == "Took":
            result.total_ms = float(right[:-3])
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

    return result

def pruebas(args):

    pods = None
    with open("pods.json") as file:
        pods = json.load(file)
    
    output_vec = [None for _ in range(args.nreq)]
    threads = []
    for i in range(args.nreq):
        t = Thread(target = lambda: launch_client(args, i, output_vec))
        t.start()
        threads.append(t)
        sleep(float(args.time) / 1000.0)
    
    for t in threads:
        t.join()
    
    times = []
    nodes = dict()
    models = dict()
    results = [parse_result(o, pods) for o in output_vec]

    for thread_id, res in enumerate(results):
        if res.route is None:
            print(f"Thread: {thread_id} failed")
        else:
            times.append(res.total_ms)
            node = res.route.split("->")[-1]
            try:
                nodes[node].append(thread_id * args.time)
            except KeyError:
                nodes[node] = [thread_id * args.time]

            try:
                models[res.model].append(thread_id * args.time)
            except KeyError:
                models[res.model] = [thread_id * args.time]
    
    # Dibujar grafica tiempo
    rango = range(0, args.nreq * args.time, args.time)
    plt.title("Tiempo de respuesta")
    plt.xlabel("Tiempo (ms)")
    plt.ylabel("Latencia (ms)")
    plt.plot(rango, times)
    plt.savefig("tiempos.png")
    
    # Dibujar grafica nodos
    plt.clf()
    fig, ax = plt.subplots(1,1) 
    y_ticks_labels = []
    for ind, (nodo, info) in enumerate(nodes.items()):
        plt.scatter(info, [ind + 1 for _ in info])
        y_ticks_labels.append(nodo)

    ax.set_yticks(list(range(1, len(y_ticks_labels) + 1)))
    ax.set_yticklabels(y_ticks_labels, fontsize=10)
    plt.savefig("nodos.png")
    
    # Dibujar grafica modelo
    plt.clf()
    plt.title("Modelo usado")
    fig, ax = plt.subplots(1,1) 
    y_ticks_labels = []
    for ind, (model, info) in enumerate(models.items()):
        plt.scatter(info, [ind + 1 for _ in info])
        y_ticks_labels.append(model)

    ax.set_yticks(list(range(1, len(y_ticks_labels) + 1)))
    ax.set_yticklabels(y_ticks_labels, fontsize=10)
    plt.savefig("modelos.png")

def main():
    
    parser = argument_parser()
    args = parser.parse_args()

    if args.gen_json:
        gen_pod_json(args.gen_json)
    else:
        pruebas(args)

if __name__ == "__main__":
    main()

