#!/usr/bin/python3

import tritonclient.http as tritonclient
from argparse import ArgumentParser, Namespace
import numpy as np
from tritonclient.utils import triton_to_np_dtype
from PIL import Image
import grpc
from time import sleep, time_ns
import random
from threading import Thread
import sys
import io
import pickle
import socket
import struct

from random import randint

def argument_parser() -> ArgumentParser:

    parser = ArgumentParser()

    parser.add_argument(
        "-u",
        "--url",
        type=str,
        required=False,
        default="localhost:8000",
        help="Inference server URL. Default is localhost:8000.",
    )

    return parser

class ModelConfig:
    def __init__(self, format, batch_size, channels, width, height, input_type, input_name, output_name):
        self.format: str = format
        self.batch_size: int = batch_size
        self.channels: int = channels
        self.width: int = width
        self.height: int = height
        self.input_type: str = input_type
        self.input_name: str = input_name
        self.output_name: str = output_name

def leer_csv_modelos(file):

    with open(file, "r") as file:
        filas = file.read().splitlines()[1:]
        filas = [ fila.split(",") for fila in filas ]
        models = dict({ fila[0]: ModelConfig(fila[1], int(fila[2]), int(fila[3]), int(fila[4]), int(fila[5]), fila[6], fila[7], fila[8]) for fila in filas })
        return models

def preprocess_image(modelo: ModelConfig, image: Image):

    #color, width, height = (input_dims[0], input_dims[2], input_dims[1]) if input_format == "FORMAT_NCHW" else (input_dims[2], input_dims[1], input_dims[0])
    
    if modelo.channels == 3:
        image = image.convert("RGB")
    else:
        print("Numero de canales no soportado:", modelo.channels)
        return

    image = image.resize(size=(modelo.width, modelo.height), resample=Image.Resampling.BILINEAR)

    nptype = triton_to_np_dtype(modelo.input_type)
    image_data: np.Array = np.array(image).astype(nptype)
    if modelo.format == "FORMAT_NCHW":
        image_data = np.transpose(image_data, (2, 0, 1))
    
    image_data = np.expand_dims(image_data, axis=0)
    if modelo.batch_size > 1:
        #print(f"Batch size == {modelo.batch_size}, se va a replicar la imágen {modelo.batch_size} veces")
        image_data = np.repeat(image_data, modelo.batch_size, axis=0)

    infer_input = tritonclient.InferInput(modelo.input_name, image_data.shape, modelo.input_type)
    infer_input.set_data_from_numpy(image_data) 
    return infer_input

def get_models(client):
    return client.get_model_repository_index()

def inference_request(client, model_name: str, model_config: ModelConfig, infer_input):
    
    response = client.infer(model_name, [infer_input])#.as_numpy("conv2d_9")
    response_data = response.as_numpy(model_config.output_name)[0]
    return response_data


def procesar_request(conn, modelos, triton_url):

    t0 = time_ns()
    bytes_input = bytes()
    while True:
        received = conn.recv(1024)
        if len(received) == 0:
            break
        bytes_input += received
    
    print("Bytes leidos: ", bytes_input[:16])
    model_name_length: int = struct.unpack(">I", bytes_input[:4])[0]
    model_name = bytes_input[4: 4 + model_name_length].decode("utf-8")
    print("Model name:", model_name)
    print("Model name length: ", model_name_length)
    model_info: ModelConfig = modelos[model_name]
    
    image = Image.open(io.BytesIO(bytes_input[4 + model_name_length:]))
    with tritonclient.InferenceServerClient(triton_url, concurrency=20) as client:
        t1 = time_ns()
        infer_input = preprocess_image(model_info, image)
        t2 = time_ns()
        resultado = inference_request(client, model_name, model_info, infer_input)
        t3 = time_ns()
        
        #b = pickle.dumps(resultado)
        #sys.stdout.buffer.write(b)
        conn.send(f"Tiempo previo (ms): {(t1 - t0) / 1_000_000}\n".encode("utf-8"))
        conn.send(f"Tiempo preprocesado (ms): {(t2 - t1) / 1_000_000}\n".encode("utf-8"))
        conn.send(f"Tiempo inferencia (ms): {(t3 - t2) / 1_000_000}\n".encode("utf-8"))
        conn.close()
        sys.stdout.flush()

if __name__ == "__main__":

    parser = argument_parser()
    ARGS = parser.parse_args()
    
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.bind(('0.0.0.0', 12345))
    sock.listen(16)

    modelos = leer_csv_modelos("modelos.csv")
    while True:
        conn, _ = sock.accept()
        t = Thread(target=lambda: procesar_request(conn,  modelos, ARGS.url))
        t.start()
