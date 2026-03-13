using Google.Protobuf;
using Google.Protobuf.Collections;
using Google.Protobuf.WellKnownTypes;

public static class RepeatedFieldExtensions {
    public static void Add(this RepeatedField<Any> list, IMessage item) {
        var packedComponent = Any.Pack(item);
        list.Add(packedComponent);
    }
}