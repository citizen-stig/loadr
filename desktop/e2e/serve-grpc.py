#!/usr/bin/env python3
"""Local helloworld.Greeter server for the gRPC example walkthrough.

Compiles the example's own protos/helloworld.proto in-process (grpcio-tools),
serves SayHello (unary) + LotsOfReplies (server-streaming), and enables server
reflection — exactly the two paths the plan exercises. Localhost-only.

    python3 serve-grpc.py protos/helloworld.proto --port 9805
"""
import sys
import tempfile
from concurrent import futures
from pathlib import Path

import grpc
from grpc_reflection.v1alpha import reflection
from grpc_tools import protoc


def main():
    argv = sys.argv[1:]
    proto = Path(argv[0]).resolve()
    port = int(argv[argv.index("--port") + 1]) if "--port" in argv else 9805

    out = tempfile.mkdtemp(prefix="loadr-grpc-")
    rc = protoc.main([
        "protoc", f"-I{proto.parent}", f"--python_out={out}", f"--grpc_python_out={out}", str(proto),
    ])
    if rc != 0:
        sys.exit("protoc failed")
    sys.path.insert(0, out)
    import helloworld_pb2 as pb2
    import helloworld_pb2_grpc as pb2_grpc

    class Greeter(pb2_grpc.GreeterServicer):
        def SayHello(self, request, context):
            return pb2.HelloReply(message=f"Hello, {request.name}!")

        def LotsOfReplies(self, request, context):
            for i in range(5):
                yield pb2.HelloReply(message=f"Hello #{i}, {request.name}!")

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=16))
    pb2_grpc.add_GreeterServicer_to_server(Greeter(), server)
    reflection.enable_server_reflection(
        (pb2.DESCRIPTOR.services_by_name["Greeter"].full_name, reflection.SERVICE_NAME), server
    )
    server.add_insecure_port(f"127.0.0.1:{port}")
    server.start()
    print(f"greeter on 127.0.0.1:{port} (reflection on)")
    server.wait_for_termination()


if __name__ == "__main__":
    main()
