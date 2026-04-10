using System.Collections.Generic;
using HalfBlind.Protobuf;
using Sirenix.OdinInspector;
using UnityEngine;

namespace HalfBlind.ItemDefinitions {
    public partial class ScriptableItemDefinition : ScriptableObject {
        [ShowInInspector] public ulong Id => ulong.Parse(name.Split('.')[0]);

        [SerializeReference] public List<ISerializedIMessage> Components = new();

        [SerializeReference] public IItemDefinitionExtension[] Extensions = { };

#if UNITY_EDITOR
        public int FindComponentOfTypeInEditor<T>(out T? cmp) where T : ISerializedIMessage {
            for (var index = 0; index < Components.Count; index++) {
                var serializedIMessage = Components[index];
                if (serializedIMessage is T typedMessage) {
                    cmp = typedMessage;
                    return index;
                }
            }

            cmp = default;
            return -1;
        }
#endif
    }
}