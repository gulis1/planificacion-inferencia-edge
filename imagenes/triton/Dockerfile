FROM nvcr.io/nvidia/tritonserver:24.03-py3
COPY model_repository /models
ENTRYPOINT ["tritonserver", "--model-repository=/models"]
#ENTRYPOINT ["bash"]
