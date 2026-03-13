using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Google.Protobuf;
using HalfBlind.Protobuf;
using ItemDefinitions;
using JetBrains.Annotations;
using Newtonsoft.Json;
using Sirenix.OdinInspector;
using UnityEditor;
using UnityEngine;

namespace BalancingEditor {
    public sealed class ItemDefinitionsJsonImport : ScriptableObject {
        [SerializeField] private ItemDefinitionsResponseEditor _exporter = null!;
        
        [SerializeField] [Sirenix.OdinInspector.FilePath]
        private string _jsonImportPath = string.Empty;

        [SerializeField]
        private DefaultAsset _directoryForNewAssets = null!;
        
        [Button]
        private void ImportItemDefinitions() {
            var newAssetPath = AssetDatabase.GetAssetPath(_directoryForNewAssets);
            
            var allProtobufTypes = AppDomain.CurrentDomain.GetAssemblies()
                .SelectMany(x => x.GetTypes())
                .Where(x => typeof(IMessage).IsAssignableFrom(x) && !x.IsAbstract)
                .ToDictionary(x => x.Name, x => x);
            
            var allSerializableTypes = AppDomain.CurrentDomain.GetAssemblies()
                .SelectMany(x => x.GetTypes())
                .Where(x => typeof(ISerializedIMessage).IsAssignableFrom(x))
                .ToDictionary(x => x.FullName, x => x);
            
            var assetPath = AssetDatabase.GetAssetPath(this);
            var directoryName = Path.GetDirectoryName(assetPath);
            var itemDefinitions = AssetDatabase.FindAssets($"t:{nameof(ScriptableObject)}", new[] { directoryName })
                .Select(AssetDatabase.GUIDToAssetPath)
                .Select(AssetDatabase.LoadAssetAtPath<ScriptableItemDefinition>)
                .Where(x => x is not null)
                .ToDictionary(x => x.Id, x => x);
            var json = File.ReadAllText(_jsonImportPath);
            var itemDefinitionJsons = JsonConvert.DeserializeObject<Dictionary<ulong, ItemDefinitionJson>>(json);
            if (itemDefinitionJsons == null) {
                Debug.LogError($"Failed to deserialize item definitions from {_jsonImportPath}", this);
                return;
            }
            foreach (var (itemId, itemDefinitionJson) in itemDefinitionJsons) {
                string expectedName = $"{itemId}{(string.IsNullOrEmpty(itemDefinitionJson.Name) ? string.Empty : $".{itemDefinitionJson.Name}")}";
                if (!itemDefinitions.TryGetValue(itemId, out var itemDefinition)) {
                    Debug.Log($"Creating new asset for ItemId:'{itemId}'", this);
                    var newItemInstance = CreateInstance<ScriptableItemDefinition>();
                    newItemInstance.name = expectedName;
                    newItemInstance.Components = itemDefinitionJson.components
                        .Select(x => ConvertToISerializedMessage(x.Key, x.Value, allSerializableTypes))
                        .Where(x => x != null)
                        .Cast<ISerializedIMessage>()
                        .ToList();
                    AssetDatabase.CreateAsset(newItemInstance, $"{newAssetPath}/{newItemInstance.name}.asset");
                    continue;                       
                }
                // Rename asset if name doesn't match
                if (!string.Equals(itemDefinition.name, expectedName)) {
                    var path = AssetDatabase.GetAssetPath(itemDefinition);
                    var error = AssetDatabase.RenameAsset(path, expectedName);
                    if (!string.IsNullOrEmpty(error)) {
                        Debug.LogError($"Failed to rename item definition {itemId}", this);
                    }
                }
                
                foreach (var (componentTypeName, componentData) in itemDefinitionJson.components) {
                    var componentTypeNameFull = $"{componentTypeName}Component";
                    if (!allProtobufTypes.TryGetValue(componentTypeNameFull, out var protoType)) {
                        Debug.LogError($"Failed to find component type {componentTypeNameFull}", this);
                        continue;
                    }

                    var componentToOverride = itemDefinition.Components.FirstOrDefault(x => {
                        var componentType = x.GetType().FullName;
                        return componentType == $"{protoType.Name}Serializable"
                               || componentType == $"{protoType.Name}SerializableClass";
                    });
                    var componentJson = JsonConvert.SerializeObject(componentData);
                    Debug.Log($"Importing {componentJson} => ItemId:'{itemId}' | {protoType.FullName}", itemDefinition);
                    if (componentToOverride != null) {
                        JsonConvert.PopulateObject(componentJson, componentToOverride, new JsonSerializerSettings {
                            ObjectCreationHandling = ObjectCreationHandling.Replace // otherwise arrays get merged
                        });
                        EditorUtility.SetDirty(itemDefinition);
                    }
                    else {
                        // Component does not exist in the item definition so we should add it
                        var convertToISerializedMessage = ConvertToISerializedMessage(componentTypeName, componentData, allSerializableTypes);
                        if (convertToISerializedMessage != null) {
                            itemDefinition.Components.Add(convertToISerializedMessage); 
                            EditorUtility.SetDirty(itemDefinition);
                        }
                        else {
                            Debug.LogError($"Failed to convert component {componentTypeName} to ISerializedIMessage", this);
                        }
                    }
                }
            }
            _exporter.ExportItemDefinitions();
            AssetDatabase.Refresh();
            AssetDatabase.SaveAssets();
            Debug.Log($"Successfully imported all item definitions to {_jsonImportPath}", this);
        }

        private ISerializedIMessage? ConvertToISerializedMessage(string componentTypeName,
            Dictionary<string, object> componentData, Dictionary<string, Type> allTypesByFullName) {
            var componentTypeFullName = $"{componentTypeName}ComponentSerializableClass";
            if (!allTypesByFullName.TryGetValue(componentTypeFullName, out var protoType)) {
                Debug.LogError($"Failed to find component type {componentTypeFullName}", this);
                return null;
            }
            var componentJson = JsonConvert.SerializeObject(componentData);
            var deserializeObject = JsonConvert.DeserializeObject(componentJson, protoType);
            return deserializeObject as ISerializedIMessage;
        }
        
        [UsedImplicitly]
        private sealed class ItemDefinitionJson {
            [JsonProperty] public string Name = string.Empty;
            [JsonProperty]
            // ReSharper disable once InconsistentNaming
            public Dictionary<string, Dictionary<string, object>> components = new();
        }
    }
}