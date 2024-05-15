#!/usr/bin/python3

import tritonclient.http as tritonclient
from argparse import ArgumentParser, Namespace
import numpy as np
from tritonclient.utils import triton_to_np_dtype
from PIL import Image
import grpc
from time import sleep
import random
from threading import Thread
import sys
import io
import pickle

from random import randint

# mask: es la array de predición que te devulve el modelo, esta array tiene que 
# contener 0,1,2,3 como valores.
# n_classes: es el número de etiquetas 
# 0 --> Fondo
# 1 --> Agua
# 2 --> Cyano
# 3 --> Rocas
def guardar_imagen_prediccion(predictions, nombre):

    shape = predictions.shape
    n_classes = 4
    colormap = np.array([[0,0,0], [50,255,255], [0, 255, 0],[100,65,23]], dtype=np.uint8)

    r = np.zeros(dtype=np.uint8, shape=(shape[1], shape[1]))
    g = np.zeros(dtype=np.uint8, shape=(shape[1], shape[1]))
    b = np.zeros(dtype=np.uint8, shape=(shape[1], shape[1]))
    rgb = np.stack([r, g, b], axis=2)

    for i in range(0, predictions.shape[1]):
        for j in range(0, predictions.shape[2]):
            categoria = np.argmax(predictions[0][i][j])
            rgb[i][j] = colormap[categoria]

    image = Image.fromarray(rgb, mode="RGB").save(nombre)

def argument_parser() -> ArgumentParser:

    parser = ArgumentParser()
    parser.add_argument(
        "-m",
        "--model-name", 
        type=str, 
        required=False, 
        help="Name of model"
    )
    parser.add_argument(
        "-i",
        "--image", 
        type=str, 
        required=False, 
        help="Path to the image"
    )
    parser.add_argument(
        "-u",
        "--url",
        type=str,
        required=False,
        default="localhost:8000",
        help="Inference server URL. Default is localhost:8000.",
    )

    parser.add_argument(
        "-o",
        "--output",
        type=str,
        required=False,
        default="output.png",
        help="File name for the output image.",
    )

    parser.add_argument(
        "--list-models",
        action="store_true",
        required=False,
        default=False,
        help="List all the models available on the server"
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
    
    # Hay que enviar un tensor de dimensiones (32, 224, 224, 3) (32 imagenes).
    # Aqui lo que hago es repetir la misma 32 veces.
    if modelo.batch_size > 1:
        #print(f"Batch size == {modelo.batch_size}, se va a replicar la imágen {modelo.batch_size} veces")
        image_data = np.expand_dims(image_data, axis=0)
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

if __name__ == "__main__":

    parser = argument_parser()
    ARGS = parser.parse_args()
    with tritonclient.InferenceServerClient(ARGS.url, concurrency=20) as client:

        if ARGS.list_models:
            for model in get_models(client):
                print(model)

        elif ARGS.model_name is not None:
            
            modelos = leer_csv_modelos("modelos.csv")
            model_info: ModelConfig = modelos[ARGS.model_name]

            image = None
            if ARGS.image is not None:
                image = Image.open(ARGS.image)
            else:
                bytes_input = sys.stdin.buffer.read()
                image = Image.open(io.BytesIO(bytes_input))
            

            infer_input = preprocess_image(model_info, image)
            resultado = inference_request(client, ARGS.model_name, model_info, infer_input)
            b = pickle.dumps(resultado)
            sys.stdout.buffer.write(b)

        else:
            parser.print_help()
    
    
