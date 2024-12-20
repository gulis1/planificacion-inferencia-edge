from cycler import cycler
from pruebas import InferenceResult
import matplotlib.pyplot as plt
import matplotlib.ticker as tkr
import sys
import json
import numpy as np
import scipy.stats as st


FROM = 0
TO = 1000  # Sin incluir la ultima


traduccion_nodos = {
    "alpinevm1": "a:edge_cpu_1",
    "alpinevm2": "b:edge_cpu_2",
    "jorinnano": "c:edge_gpu_1",
    "artecslab003": "d:fog_cpu_1",
    "orin2": "e:fog_gpu_1",
    "ocejon": "f:cloud_gpu_1",
    "marbore": "g:cloud_gpu_2"
}



def int_confianza(datos, confianza):

    mean = np.mean(datos)
    zscore = st.zscore(datos)
    filtrados = [t for i, t in enumerate(datos) if abs(zscore[i]) < 3]
    return np.std(filtrados)


def parse_json(filename):
    
    results = []
    time = None
    nreq = None
    with open(filename) as file:
        content = json.load(file)
        time = content["time"]
        nreq = content["nreq"]
        for row in content["results"]:
            res = InferenceResult()
            res.__dict__ = row
            results.append(res)

    return time, nreq, results

def dibujar_graficas(nodes, times, models, time, nreq):

    fig, ax = plt.subplots(1,1) 

    # Dibujar grafica tiempo
    node_names = sorted(nodes.keys())
    for nodo in node_names:
        info = nodes[nodo]
        plt.scatter(info[0], info[1], label=nodo.split(":")[1], zorder=2)
    

    ## Dibujar grafica nodos
    #plt.clf()
    #fig, ax = plt.subplots(1,1) 
    #node_names = sorted(nodes.keys())
    #node_names_inserted = list(nodes.keys())
    #for nodo in node_names:
    #    info = nodes[nodo]
    #    plt.scatter(info[0], [node_names_inserted.index(nodo) + 1 for _ in info[0]], zorder=2)

    #ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, time * nreq, time))))
    #ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    #ax.set_yticks(list(range(1, len(node_names_inserted) + 1)))
    #ax.set_yticklabels(node_names_inserted, fontsize=10)
    #plt.grid(axis="x", zorder=1, which="both")
    #
    #plt.title("Nodo")
    #fig = plt.gcf()
    #fig_width = max(len(times) * 0.5, 14) * cm + 3
    #fig.set_size_inches(fig_width, 6)
    #fig.savefig("nodos.png", dpi=100)
    
    # Dibujar grafica modelo
    plt.clf()
    fig, ax = plt.subplots(1,1) 
    y_ticks_labels = []
    model_names = sorted(models.keys())
    for ind, model in enumerate(model_names):
        info = models[model]
        plt.scatter(info, [ind + 1 for _ in info], zorder=2)
        y_ticks_labels.append(model)

    ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, time * nreq, time))))
    ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    ax.set_yticks(list(range(1, len(y_ticks_labels) + 1)))
    ax.set_yticklabels(y_ticks_labels, fontsize=10)
    plt.grid(axis="x", zorder=1, which="both")

    plt.title("Modelo usado")
    fig = plt.gcf()
    fig_width = max(len(times) * 0.5, 14) * cm + 3
    fig.set_size_inches(fig_width, 6)
    fig.savefig("modelos.png", dpi=100)

    plt.clf()
    fig, ax = plt.subplots(figsize=(12, 8))
    x = np.arange(len(nodes))
    #X = list(nodes)
    node_names = sorted(nodes)
    Y = [len(nodes[node][0]) for node in node_names]
    plt.bar(x, Y, width=0.3)
    Y2 = [sum(nodes[node][1]) / len(nodes[node][1]) for node in node_names]
    ax.set_ylabel("Amount of requests", color="tab:blue")
    ax2 = ax.twinx()
    ax2.set_ylabel("Average response time (ms)", color="tab:red")
    plt.bar(x + 0.3, Y2, width=0.3, color="tab:red")
    plt.title("IPtables: random")
    ax.set_xticks(x + 0.3 / 2)
    ax.set_xticklabels([node.split(":")[1] for node in node_names])
    plt.savefig("barras_nodos.png")

if __name__ == "__main__":

    fig, ax = plt.subplots(1,1) 
    nodes = dict()
    algoritmos = dict()
    
    linestyle_cycler = (cycler(linestyle=['-','--',':','-.']))
    ax.set_prop_cycle(linestyle_cycler)
    plt.yscale("log")
    for arg in sys.argv[1:]:

        time, nreq, results = parse_json(arg)
        if TO - FROM > nreq:
            print("TO - FROM > Número total de peticiones")
            exit(1)

        nreq = TO - FROM
        results = results[FROM:TO]
        time = 1000
        times = []
        models = dict()
        algoritmo = arg.split("/")[-1].replace(".json", "")
        for thread_id, res in enumerate(results):
            
            if res.route is None:
                #print(f"Thread: {thread_id} failed")
                times.append(None)
            else:
                #print(f"Thread {thread_id}: {res.total_ms}ms (inf: {res.t_inferencia}ms, pproc: {res.t_pprocesado}ms), {res.route}")
                times.append(res.total_ms)
                node = traduccion_nodos[res.route.split("->")[-1]]
                try:
                    nodes[node][0].append(thread_id * time)
                    nodes[node][1].append(res.total_ms)
                    nodes[node][2].append(algoritmo)
                except KeyError:
                    nodes[node] = ([thread_id * time], [res.total_ms], [])

                try:
                    models[res.model].append(thread_id * time)
                except KeyError:
                    models[res.model] = [thread_id * time]


        rango = range(0, nreq * time, time)
        line = ax.plot(rango, times, zorder=1, linewidth=1, color="gray")
        algoritmos[algoritmo] = times


    # Dibujar grafica tiempo
    ax.set_prop_cycle(None)
    node_names = sorted(nodes.keys())
    for nodo in node_names:
        info = nodes[nodo]
        ax.scatter(info[0], info[1], zorder=2, s=30)
        
    cm = 1/2.54

    plt.title("Tiempo de respuesta")
    plt.xlabel("Tiempo (ms)")
    plt.ylabel("Latencia (ms)")
    
    legend1 = plt.legend(list(algoritmos.keys()) + [node.split(":")[1] for node in node_names], loc="upper right", title="Nodos")
    ax.add_artist(legend1)
    
    fig = plt.gcf()
    prev_width = fig.get_figwidth()
    prev_height = fig.get_figheight()
    fig_width = max(len(times) * 0.5, 14) * cm + 3
    fig.set_size_inches(fig_width, 6)
    ax.xaxis.set_minor_locator(tkr.FixedLocator(list(range(0, time * nreq, time))))
    ax.xaxis.set_major_locator(tkr.MaxNLocator(10))
    #plt.grid(axis="x", zorder=1, which="both")
    fig.savefig("tiempos.png", dpi=100)


    fig.set_size_inches(prev_width, prev_height)
    
    
    for algoritmo, tiempos in algoritmos.items():
        print(f"{algoritmo}, {len([t for t in tiempos if t is None])}")

    plt.clf()
    ax = plt.gca()
    ax.set_ylim(0, 1000)
    plt.xlabel("Algoritmo")
    plt.ylabel("Tiempo respuesta promedio")
    plt.bar(algoritmos.keys(), [sum([t for t in tiempo if t is not None]) / len([t for t in tiempo if t is not None]) for tiempo in algoritmos.values()])
    plt.errorbar(algoritmos.keys(), [sum([t for t in tiempo if t is not None]) / len([t for t in tiempo if t is not None]) for tiempo in algoritmos.values()], [int_confianza([t for t in tiempo if t is not None], 0.99) for tiempo in algoritmos.values()], linestyle="None", color="black", capsize=5)
    ax.set_xticks
    plt.savefig("desviaciones.png")
       
    dibujar_graficas(nodes, times, models, time, nreq)
