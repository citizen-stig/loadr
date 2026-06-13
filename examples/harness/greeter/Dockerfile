FROM python:3.12-slim
RUN pip install --no-cache-dir grpcio grpcio-reflection
COPY greeter.py /greeter.py
EXPOSE 50051
CMD ["python","/greeter.py"]
