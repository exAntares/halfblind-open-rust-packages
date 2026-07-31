using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Google.Protobuf;
using HalfBlind.ItemDefinitions;
using Newtonsoft.Json;
using ProtobufItemdefinition;
using Sirenix.OdinInspector;
using UnityEditor;
using UnityEngine;

namespace BalancingEditor {
    [CreateAssetMenu]
    public sealed class ItemDefinitionsResponseEditor : ScriptableObject {
        [SerializeField] [Sirenix.OdinInspector.FilePath]
        private string _path = string.Empty;
        
        [SerializeField] private ExportDebugStyle _exportDebugStyle = ExportDebugStyle.SingleFileExport;
        
        [Button]
        public void ExportItemDefinitions() {
            var assetPath = AssetDatabase.GetAssetPath(this);
            var directoryName = Path.GetDirectoryName(assetPath);
            var response = new ItemDefinitionsResponse();
            var sortedDictionary = new SortedDictionary<ulong, Dictionary<string, IMessage>>();
            var scriptableItemDefinitions = AssetDatabase.FindAssets($"t:{nameof(ScriptableObject)}", new[] { directoryName })
                .Select(AssetDatabase.GUIDToAssetPath)
                .Select(AssetDatabase.LoadAssetAtPath<ScriptableItemDefinition>)
                .Where(x => x is not null)
                .ToArray();
            var itemDefinitions = scriptableItemDefinitions
                .Select(x => {
                    var itemDefinition = new ItemDefinition {
                        Id = x.Id,
                    };
                    var messages = x.Components.Select(scriptableProtobufMessage => {
                        if (scriptableProtobufMessage == null) {
                            Debug.LogError($"Null component found in item definition {x.Id}", this);
                            throw new NullReferenceException();
                        }
                        try {
                            return scriptableProtobufMessage.GetMessage();
                        }
                        catch (Exception e) {
                            Debug.LogError("Failed to serialize component " + scriptableProtobufMessage.GetType().Name + " for item definition " + x.Id + ": " + e, x);
                            throw;
                        }
                    }).ToList();
                    sortedDictionary[x.Id] = messages.ToDictionary(message => message.GetType().Name, message => message);
                    foreach (var message in messages) {
                        itemDefinition.AnyComponents.Add(message);
                    }
                    return itemDefinition;
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
            response.Definitions.Add(itemDefinitions);
            var byteArray = response.ToByteArray();
            File.WriteAllBytes(_path, byteArray);
            switch (_exportDebugStyle) {
                case ExportDebugStyle.SingleFileExport:
                    var jsonPath = _path.Replace(Path.GetExtension(_path), ".json");
                    File.WriteAllText(jsonPath, JsonConvert.SerializeObject(sortedDictionary, Formatting.Indented));
                    break;
                case ExportDebugStyle.MultiFileExport:
                    var exportDirectory = Path.GetDirectoryName(_path);
                    foreach (var x1 in sortedDictionary) {
                        var itemDefinitionPath = Path.Combine(exportDirectory, $"Item_{x1.Key}.json");
                        File.WriteAllText(itemDefinitionPath, JsonConvert.SerializeObject(x1.Value, Formatting.Indented));
                    }
                    break;
            }
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
        public enum ExportDebugStyle {
            DoNotExportDebugFiles,
            SingleFileExport,
            MultiFileExport,
        }
    }
}
