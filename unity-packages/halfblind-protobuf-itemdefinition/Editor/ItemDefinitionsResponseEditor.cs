using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Google.Protobuf;
using HalfBlind.ItemDefinitions;
using ProtobufItemdefinition;
using Sirenix.OdinInspector;
using UnityEditor;
using UnityEngine;
using File = UnityEngine.Windows.File;

namespace BalancingEditor {
    [CreateAssetMenu]
    public sealed class ItemDefinitionsResponseEditor : ScriptableObject {
        [SerializeField] [Sirenix.OdinInspector.FilePath]
        private string _path = string.Empty;

        [Button]
        public void ExportItemDefinitions() {
            var assetPath = AssetDatabase.GetAssetPath(this);
            var directoryName = Path.GetDirectoryName(assetPath);
            var items = new ItemDefinitionsResponse();
            var scriptableItemDefinitions = AssetDatabase.FindAssets($"t:{nameof(ScriptableObject)}", new[] { directoryName })
                .Select(AssetDatabase.GUIDToAssetPath)
                .Select(AssetDatabase.LoadAssetAtPath<ScriptableItemDefinition>)
                .Where(x => x is not null)
                .ToArray();
            var itemDefinitions = scriptableItemDefinitions
                .Select(x => {
                    var result = new ItemDefinition {
                        Id = x.Id,
                    };
                    foreach (var scriptableProtobufMessage in x.Components) {
                        if (scriptableProtobufMessage == null) {
                            Debug.LogError($"Null component found in item definition {x.Id}", this);
                            throw new NullReferenceException();
                        }
                        try {
                            var message = scriptableProtobufMessage.GetMessage();
                            result.AnyComponents.Add(message);
                        }
                        catch (Exception e) {
                            Debug.LogError("Failed to serialize component " + scriptableProtobufMessage.GetType().Name + " for item definition " + x.Id + ": " + e, x);
                            throw;
                        }
                    }
                    return result;
                })
                .ToArray();
            // Check for duplicates
            var allDefinitions = itemDefinitions.ToDictionary(x => x.Id, x => x);
            var errors = ValidateComponentRefs(scriptableItemDefinitions, allDefinitions);
            if (errors.Count > 0) {
                foreach (var error in errors) {
                    Debug.LogError(error.error, error.owner);    
                }
                throw new Exception("Found errors in item definitions. See log for details.");
            }
            items.Definitions.Add(itemDefinitions);
            var byteArray = items.ToByteArray();
            File.WriteAllBytes(_path, byteArray);
            AssetDatabase.Refresh();
            Debug.Log($"Successfully exported all item definitions to {_path}", this);
        }

        private static List<(string error, ScriptableItemDefinition owner)> ValidateComponentRefs(
            ScriptableItemDefinition[] scriptableItemDefinitions,
            Dictionary<ulong, ItemDefinition> allDefinitions
            ) {
            var result = new List<(string, ScriptableItemDefinition)>();
            foreach (var definition in scriptableItemDefinitions) {
                foreach (var component in definition.Components) {
                    if (component != null) {
                        var so = new SerializedObject(definition);
                        foreach (var itemRef in EnumerateItemDefinitionRefs(so)) {
                            if (!allDefinitions.TryGetValue(itemRef.Id, out var itemDefinition)) {
                                result.Add(($"ItemDefinition Ref does not exist for id '{itemRef.Id}' at item definition: {definition.Id}\n", definition));
                            }
                        }
                    }
                    else {
                        result.Add(($"Component is null for item definition {definition.Id}\n", definition));
                    }
                }
            }
            return result;
        }
        
        private static IEnumerable<(string PropertyPath, ulong Id)> EnumerateItemDefinitionRefs(SerializedObject serializedObject) {
            var iterator = serializedObject.GetIterator();
            var enterChildren = true;

            while (iterator.Next(enterChildren)) {
                enterChildren = true;

                if (iterator.propertyType != SerializedPropertyType.Generic) {
                    continue;
                }

                object boxedValue;
                try {
                    boxedValue = iterator.boxedValue;
                }
                catch {
                    continue;
                }

                switch (boxedValue) {
                    case ItemDefinitionRefSerializable itemRef:
                        yield return (iterator.propertyPath, itemRef.Id);
                        break;

                    case ItemDefinitionRefSerializableClass itemRefClass when itemRefClass != null:
                        yield return (iterator.propertyPath, itemRefClass.Id);
                        break;
                }
            }
        }
    }
}
