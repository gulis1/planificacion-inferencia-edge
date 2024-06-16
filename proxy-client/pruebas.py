from argparse import ArgumentParser
import subprocess
from threading import Thread
from time import sleep
import json
import matplotlib.pyplot as plt
import matplotlib.ticker as tkr
import numpy as np
import pickle

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
    
    times = []
    nodes = dict()
    models = dict()
    for thread_id, res in enumerate(results):
        if res.route is None:
            print(f"Thread: {thread_id} failed")
            times.append(None)
        else:
            print(f"Thread {thread_id}: {res.total_ms}ms (inf: {res.t_inferencia}ms, pproc: {res.t_pprocesado}ms), {res.route}")
            times.append(res.total_ms)
            node = res.route.split("->")[-1]
            try:
                nodes[node][0].append(thread_id * args.time)
                nodes[node][1].append(res.total_ms)
            except KeyError:
                nodes[node] = ([thread_id * args.time], [res.total_ms])

            try:
                models[res.model].append(thread_id * args.time)
            except KeyError:
                models[res.model] = [thread_id * args.time]
    
    fig, ax = plt.subplots(1,1) 

    # Dibujar grafica tiempo
    node_names = sorted(nodes.keys())
    for nodo in node_names:
        info = nodes[nodo]
        plt.scatter(info[0], info[1], label=nodo, zorder=2)
    
    cm = 1/2.54

    rango = range(0, args.nreq * args.time, args.time)
    plt.title("Tiempo de respuesta")
    plt.xlabel("Tiempo (ms)")
    plt.ylabel("Latencia (ms)")
    plt.plot(rango, times, zorder=1)
    
    plt.legend()
    fig = plt.gcf()
    fig_width = max(len(times) * 0.5, 14) * cm + 3
    fig.set_size_inches(fig_width, 6)
    ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, args.time * args.nreq, args.time))))
    ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    plt.grid(axis="x", zorder=1, which="both")
    fig.savefig("tiempos.png", dpi=100)


    # Dibujar grafica nodos
    plt.clf()
    fig, ax = plt.subplots(1,1) 
    node_names = sorted(nodes.keys())
    node_names_inserted = list(nodes.keys())
    for nodo in node_names:
        info = nodes[nodo]
        plt.scatter(info[0], [node_names_inserted.index(nodo) + 1 for _ in info[0]], zorder=2)

    ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, args.time * args.nreq, args.time))))
    ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    ax.set_yticks(list(range(1, len(node_names_inserted) + 1)))
    ax.set_yticklabels(node_names_inserted, fontsize=10)
    plt.grid(axis="x", zorder=1, which="both")
    
    plt.title("Nodo")
    fig = plt.gcf()
    fig_width = max(len(times) * 0.5, 14) * cm + 3
    fig.set_size_inches(fig_width, 6)
    fig.savefig("nodos.png", dpi=100)
    
    # Dibujar grafica modelo
    plt.clf()
    fig, ax = plt.subplots(1,1) 
    y_ticks_labels = []
    model_names = sorted(models.keys())
    for ind, model in enumerate(model_names):
        info = models[model]
        plt.scatter(info, [ind + 1 for _ in info], zorder=2)
        y_ticks_labels.append(model)

    ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, args.time * args.nreq, args.time))))
    ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    ax.set_yticks(list(range(1, len(y_ticks_labels) + 1)))
    ax.set_yticklabels(y_ticks_labels, fontsize=10)
    plt.grid(axis="x", zorder=1, which="both")

    plt.title("Modelo usado")
    fig = plt.gcf()
    fig_width = max(len(times) * 0.5, 14) * cm + 3
    fig.set_size_inches(fig_width, 6)
    fig.savefig("modelos.png", dpi=100)

def main():
    
    parser = argument_parser()
    args = parser.parse_args()

    if args.gen_json:
        gen_pod_json(args.gen_json)
    else:
        pruebas(args)

if __name__ == "__main__":
    main()

