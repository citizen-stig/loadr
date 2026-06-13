import grpc, time
from concurrent import futures
def _varint(n):
    out=b""
    while True:
        x=n&0x7f; n>>=7; out+=bytes([x|0x80 if n else x])
        if not n: return out
def pb_string(f,s):
    b=s.encode(); return bytes([f<<3|2])+_varint(len(b))+b
def pb_read_string(data):
    if not data or data[0]!=0x0a: return ""
    i=1; ln=0; sh=0
    while True:
        byte=data[i]; i+=1; ln|=(byte&0x7f)<<sh
        if not byte&0x80: break
        sh+=7
    return data[i:i+ln].decode("utf-8","replace")
def say_hello(req,ctx): return pb_string(1,"Hello "+pb_read_string(req))
def lots(req,ctx):
    name=pb_read_string(req)
    for i in range(3): yield pb_string(1,f"Hello {name} #{i}")
handlers={"SayHello":grpc.unary_unary_rpc_method_handler(say_hello),
          "LotsOfReplies":grpc.unary_stream_rpc_method_handler(lots)}
srv=grpc.server(futures.ThreadPoolExecutor(max_workers=16))
srv.add_generic_rpc_handlers((grpc.method_handlers_generic_handler("helloworld.Greeter",handlers),))
from grpc_reflection.v1alpha import reflection
reflection.enable_server_reflection(("helloworld.Greeter",reflection.SERVICE_NAME),srv)
srv.add_insecure_port("0.0.0.0:50051"); srv.start()
print("real gRPC greeter + reflection on 50051",flush=True)
while True: time.sleep(3600)
