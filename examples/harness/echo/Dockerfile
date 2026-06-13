FROM python:3.12-slim
RUN pip install --no-cache-dir websockets
COPY echo_server.py /echo_server.py
EXPOSE 8081 8082 7000 8125/udp
CMD ["python","/echo_server.py"]
