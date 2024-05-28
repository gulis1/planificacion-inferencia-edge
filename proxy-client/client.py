import struct
import socket
from argparse import ArgumentParser
from uuid import uuid4
import pickle
import numpy as np
from PIL import Image
from time import time_ns
import subprocess

NAMESPACE = "kube-triton"

def get_pods():

    service_uuid = subprocess.check_output(f"sudo kubectl --namespace={NAMESPACE} describe tservice | grep UID", shell=True).decode("utf8").split()[1]
    output = subprocess.check_output([
        "sudo",
        "kubectl",
        f"--namespace={NAMESPACE}",
        "get",
        "pods",
        "-o",
        "custom-columns=NODE:.spec.nodeName,PodUID:.metadata.uid,status:status.phase"
    ]).decode("utf8")

    lines = [ line.split() for line in output.splitlines() ]
    pods = { line[1]:line[0] for line in lines if line[-1] == "Running" }

    return service_uuid, pods

def buscar_ruta(req_id):
    
    service_uuid, pod_dict = get_pods()
    output = subprocess.check_output(f"sudo kubectl --namespace {NAMESPACE} get pods -o custom-columns=NAME:.metadata.name,NODE:.spec.nodeName,PodUID:.metadata.uid,status:status.phase | grep Running", shell=True).decode("utf-8")
    lines = [line.split() for line in output.splitlines()]
    pods = [(line[0], line[1]) for line in lines]
    
    ruta = []
    while True:
        for (pod, node) in pods:

            # Buscar logs
            logs = subprocess.check_output(f"sudo kubectl --namespace kube-triton logs pod/{pod} triton-proxy | grep PROXY_DEBUG | tail", shell=True).decode("utf-8")
            for line in logs.splitlines():
                info = line.split()[-3:]
                request_id = info[0]
                jump = int(info[1])
                try:
                    if request_id != str(req_id):
                        continue
                    
                    if info[2].startswith("model"):
                        if jump == 0 and len(ruta) == 0:
                            ruta.append(node)
                            print(info[2])
                            return ruta
                        elif len(ruta) == (jump + 1):
                            print(info[2])
                            return ruta
                    else:
                        target = pod_dict[info[2]] if info[2] != "localhost" else info[2]
                        if jump == 0 and len(ruta) == 0:
                            ruta += [node, target]
                        elif len(ruta) == (jump + 1):
                            ruta.append(target)

                except KeyError:
                    print("error", info[2])
                    pass

            # Buscar timeouts
            logs = subprocess.check_output(f"sudo kubectl --namespace kube-triton logs pod/{pod} triton-proxy | grep 'Timeout expired'| tail", shell=True).decode("utf-8")
            for line in logs.splitlines():
                failed_request_id = line.split()[-1]
                if failed_request_id == req_id:
                    return ruta 

def argument_parser() -> ArgumentParser:

    parser = ArgumentParser()
    parser.add_argument(
        "-u",
        "--url",
        type=str,
        required=True,
        help="Inference server URL. Default is localhost:8000.",
    )

    parser.add_argument(
        "-i",
        "--image",
        type=str,
        required=True,
        help="Image path",
    )

    parser.add_argument(
        "-a",
        "--accuracy",
        type=int,
        required=False,
        default=0,
        help="Accuracy",
    )

    parser.add_argument(
        "-p",
        "--priority",
        type=int,
        required=False,
        default=0,
        help="Priority",
    )

    return parser

def guardar_imagen_prediccion(predictions, nombre):

    shape = predictions.shape

    n_classes = 4
    colormap = np.array([[0,0,0], [50,255,255], [0, 255, 0],[100,65,23]], dtype=np.uint8)

    r = np.zeros(dtype=np.uint8, shape=(shape[0], shape[1]))
    g = np.zeros(dtype=np.uint8, shape=(shape[0], shape[1]))
    b = np.zeros(dtype=np.uint8, shape=(shape[0], shape[1]))
    rgb = np.stack([r, g, b], axis=2)

    for i in range(0, predictions.shape[0]):
        for j in range(0, predictions.shape[1]):
            categoria = np.argmax(predictions[i][j])
            rgb[i][j] = colormap[categoria]

    image = Image.fromarray(rgb, mode="RGB").save(nombre)

def main():
    parser = argument_parser()
    args = parser.parse_args()

    if args.priority < 0 or args.priority > 5:
        print("Priority must be betweeo 0 and 5")
        return

    if args.accuracy < 0 or args.accuracy > 5:
        print("Accuracy must be betweeo 0 and 5")
        return

    host = args.url.split(":")[0]
    port = int(args.url.split(":")[1])

    uuid = uuid4()
    print("Request id:", uuid)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:

        sock.connect((host, port))
        file = open(args.image, "rb")
        img = file.read()
        file.close()

        sock.send(uuid.bytes)
        sock.send(struct.pack(">I", 0))
        sock.send(struct.pack(">B", args.priority))
        sock.send(struct.pack(">B", args.accuracy))
        sock.send(struct.pack(">Q", len(img)))
        sock.send(img)
        
        response = bytes()
        t1 = time_ns()
        while True:
            received = sock.recv(1024)
            if len(received) == 0:
                break
            response += received

        t2 = time_ns()
        try:
            #x = pickle.loads(response)
            #print(x.shape)
            print(response.decode("utf-8"))
        except Exception as e:
            print("Error cliente:", e)
        print(f"Took: {(t2 - t1) / 1_000_000} ms")
        #ruta = buscar_ruta(uuid)
        #print("Ruta:", " -> ".join(ruta))

if __name__ == "__main__":
    main()



