import pydot
import tkinter as tk
from time import sleep
from PIL import Image, ImageTk
from datetime import datetime
import requests
import subprocess
from sys import argv
from threading import Thread

def get_pods(namespace):

    service_uuid = subprocess.check_output(f"sudo kubectl --namespace={namespace} describe tservice | grep UID", shell=True).decode("utf8").split()[1]
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
    pods = { line[1]:line[0] for line in lines if line[-1] == "Running" }

    return service_uuid, pods

def scrap_logs(namespace, pod_dict):

    def print_ruta(id, lista_nodos, success=True):
        id_simple = id.split("-")[0]
        ruta = " -> ".join(lista_nodos)
        if success:
            print(f"{id_simple}: {ruta}")
        else:
            print(f"{id_simple} (TIMEOUT): {ruta}")

    output = subprocess.check_output(f"sudo kubectl --namespace {namespace} get pods -o custom-columns=NAME:.metadata.name,NODE:.spec.nodeName,PodUID:.metadata.uid,status:status.phase | grep Running", shell=True).decode("utf-8")
    lines = [line.split() for line in output.splitlines()]
    pods = [(line[0], line[1]) for line in lines]

    following = {}
    completadas = set()
    while True:
        for (pod, node) in pods:

            # Buscar logs
            logs = subprocess.check_output(f"sudo kubectl --namespace kube-triton logs pod/{pod} triton-proxy | grep PROXY_DEBUG | tail", shell=True).decode("utf-8")
            for line in logs.splitlines():
                info = line.split()[-3:]
                request_id = info[0]
                jump = int(info[1])
                try:
                    target = pod_dict[info[2]] if info[2] != "localhost" else "localhost"

                    if jump == 0 and request_id not in following and request_id not in completadas:
                        if target == "localhost":
                            print_ruta(request_id, [node])
                            completadas.add(request_id)
                        else:
                            following[request_id] = [node, target]
                    elif request_id in following and len(following[request_id]) == (jump + 1):
                        if target == "localhost":
                            print_ruta(request_id, following[request_id])
                            completadas.add(request_id)
                            following.pop(request_id)
                        else:
                            following[request_id].append(target)
                except KeyError:
                    pass

            # Buscar timeouts
            logs = subprocess.check_output(f"sudo kubectl --namespace kube-triton logs pod/{pod} triton-proxy | grep 'Timeout expired'| tail", shell=True).decode("utf-8")
            for line in logs.splitlines():
                failed_request_id = line.split()[-1]
                if failed_request_id in following:
                    print_ruta(failed_request_id, following[failed_request_id], success=False)
                    completadas.add(failed_request_id)
                    following.pop(failed_request_id)

def main(url, namespace):
    
    service_uuid, pods = get_pods(namespace)
    
    #t = Thread(target=lambda: scrap_logs(namespace, pods))
    #t.start()

    root = tk.Tk()
    root.minsize(800, 600)
    panel = tk.Label(root)
    panel.pack(side = "bottom", fill = "both", expand = "yes")
    
    while True:
        
        response = None
        try:
            response = requests.get(f"http://{url}/{service_uuid}")
            if response.status_code != 200:
                root.wm_title(f"Request failed with error {reponse.status_code}")
            else:
                data = response.text
                for key, value in pods.items():
                    data = data.replace(key, value)
                g: pydot.Dot = pydot.graph_from_dot_data(data)[0]
                g.write_png("output.png")
                
                img = ImageTk.PhotoImage(Image.open("output.png"))
                panel.image = img
                panel.configure(image=img)
                root.wm_title(f"Última actualización: {datetime.now().strftime('%H:%M:%S')}")
        except Exception as e:
            print(e)
            root.wm_title(f"Could not connect to server.")
        root.update_idletasks()
        root.update()
        sleep(5)

if __name__ == "__main__":
    
    if len(argv) != 3:
        print("Usage: visualice.py <server> <namespace>")
        exit(1)
    
    main(argv[1], argv[2])
