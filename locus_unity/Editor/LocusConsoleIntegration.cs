// Adds "Send to Locus" affordances to the Unity Console window:
//   - Menu items under "Window/Locus Console" (send all / send selected).
//   - A toolbar button rendered on top of the Unity Console window via reflection.
//   - Right-click context menu entries on the Console list rows.
//
// All UnityEditor.ConsoleWindow / ListViewState accesses are reflection-based
// because those types are not part of the public UnityEditor API.

using UnityEditor;
using UnityEngine;

using System;
using System.Collections.Generic;
using System.Reflection;
using System.Text;

namespace Locus
{
    [InitializeOnLoad]
    internal static class LocusConsoleIntegration
    {
        private const string ToolbarButtonText = "Send to Locus";
        private const string ToolbarButtonTooltip =
            "Send Unity Console entries to Locus. Uses the current selection when present, otherwise sends the full console tail.";

        private static readonly Type ConsoleWindowType =
            FindEditorType("UnityEditor.ConsoleWindow", "UnityEditorInternal.ConsoleWindow");
        private static readonly Type ListViewStateType =
            FindEditorType("UnityEditor.IMGUI.Controls.ListViewState", "UnityEditor.ListViewState");
        private static readonly Type LogEntriesType =
            FindEditorType("UnityEditor.LogEntries", "UnityEditorInternal.LogEntries");

        private static MethodInfo _logEntriesGetCount;
        private static MethodInfo _logEntriesStartGettingEntries;
        private static MethodInfo _logEntriesEndGettingEntries;
        private static MethodInfo _logEntriesGetEntryInternal;
        private static MethodInfo _logEntriesGetLinesAndMode;

        private static FieldInfo _consoleWindowListViewField;
        private static FieldInfo _listViewSelectedItemsField;
        private static FieldInfo _listViewRowField;
        private static MethodInfo _listViewGetTotalRows;

        private static bool _bindingsResolved;
        private static bool _bindingFailed;
        private static double _nextBindingsRetryAt;
        private const double BindingsRetryIntervalSeconds = 5d;

        private static readonly List<int> SelectionScratch = new List<int>(16);
        private static readonly List<int> SelectionSortedScratch = new List<int>(16);

        static LocusConsoleIntegration()
        {
            ResolveBindings();
        }

        public static void SendAllToLocus()
        {
            DispatchConsoleTextToLocus(null, "Send to Locus: full console");
        }

        public static void SendSelectedToLocus()
        {
            int[] selectedRows = TryGetSelectedConsoleEntryIndices();
            string summary = selectedRows != null && selectedRows.Length > 0
                ? "Send to Locus: " + selectedRows.Length + " selected"
                : "Send to Locus: no selection, falling back to full console";
            DispatchConsoleTextToLocus(selectedRows, summary);
        }

        // ─── Toolbar / context menu integration ───

        /// <summary>
        /// Rendered via a delegate hooked on the Console window. Returns true if it consumed the
        /// toolbar layout pass so other consumers can stack after it.
        /// </summary>
        internal static void HandleLocusConsoleSendButtonMouseDown()
        {
            SendSelectedToLocus();
        }

        internal static bool DrawConsoleToolbarButton()
        {
            GUILayout.Space(4f);
            GUIContent content = new GUIContent(ToolbarButtonText, ToolbarButtonTooltip);
            if (GUILayout.Button(content, EditorStyles.miniButton, GUILayout.Width(96f)))
            {
                SendSelectedToLocus();
                return true;
            }
            return false;
        }

        internal static void AppendConsoleContextMenu(GenericMenu menu)
        {
            if (menu == null) return;
            menu.AddItem(new GUIContent("Send Selected to Locus"), false, () => SendSelectedToLocus());
            menu.AddItem(new GUIContent("Send All to Locus"), false, () => SendAllToLocus());
        }

        // ─── Core dispatch ───

        private static void DispatchConsoleTextToLocus(int[] selectedRows, string actionLabel)
        {
            try
            {
                string payloadJson;
                if (selectedRows != null && selectedRows.Length > 0)
                {
                    payloadJson = LocusBridge.BuildConsoleTextPayloadJsonForSelection(selectedRows);
                }
                else
                {
                    payloadJson = LocusBridge.BuildConsoleTextPayloadJson();
                }

                ConsoleTextEnvelope envelope = ParseConsoleTextEnvelope(payloadJson);
                if (envelope == null || (string.IsNullOrEmpty(envelope.text) && (envelope.entries == null || envelope.entries.Length == 0)))
                {
                    Debug.LogWarning("[Locus] Console has no entries to send.");
                    return;
                }

                // The dispatch path mirrors LocusBridge.ConsoleSourceSelected / "unity-console-selected"
                // so downstream consumers (Locus desktop, agent prompt builder) can tell apart
                // selection-scoped payloads from full-console dumps.
                string error;
                bool ok = LocusEditorWindow.SendConsoleTextEntries(
                    envelope.text,
                    envelope.entries,
                    envelope.title,
                    envelope.source,
                    out error);
                if (!ok)
                {
                    Debug.LogWarning("[Locus] " + actionLabel + " failed: " + error);
                    return;
                }
                int count = envelope.entries != null ? envelope.entries.Length : 0;
                Debug.Log("[Locus] " + actionLabel + " sent " + count + " entries.");
            }
            catch (Exception ex)
            {
                Debug.LogError("[Locus] Failed to dispatch console text: " + ex);
            }
        }

        private static ConsoleTextEnvelope ParseConsoleTextEnvelope(string json)
        {
            if (string.IsNullOrEmpty(json))
                return null;
            try
            {
                return JsonUtility.FromJson<ConsoleTextEnvelope>(json);
            }
            catch
            {
                return null;
            }
        }

        [Serializable]
        private sealed class ConsoleTextEnvelope
        {
            public string text;
            public LocusEditorWindow.ConsoleTextEntryDto[] entries;
            public string title;
            public string source;
        }

        // ─── Selection extraction ───

        private static int[] TryGetSelectedConsoleEntryIndices()
        {
            if (!ResolveBindings() || ConsoleWindowType == null || ListViewStateType == null)
                return null;

            EditorWindow consoleWindow = FindOpenConsoleWindow();
            if (consoleWindow == null)
                return null;

            object listView = TryGetConsoleListView(consoleWindow);
            if (listView == null)
                return null;

            bool[] selectedItems = TryGetListViewSelectedItems(listView);
            if (selectedItems == null || selectedItems.Length == 0)
                return null;

            SelectionScratch.Clear();
            for (int i = 0; i < selectedItems.Length; i++)
            {
                if (selectedItems[i])
                    SelectionScratch.Add(i);
            }

            if (SelectionScratch.Count == 0)
            {
                int focusedRow = TryGetListViewFocusedRow(listView);
                if (focusedRow >= 0 && focusedRow < selectedItems.Length)
                    SelectionScratch.Add(focusedRow);
            }

            if (SelectionScratch.Count == 0)
                return null;

            SelectionSortedScratch.Clear();
            SelectionSortedScratch.AddRange(SelectionScratch);
            SelectionSortedScratch.Sort();
            return SelectionSortedScratch.ToArray();
        }

        private static bool[] TryGetListViewSelectedItems(object listView)
        {
            if (listView == null) return null;
            try
            {
                if (_listViewSelectedItemsField == null)
                {
                    _listViewSelectedItemsField = ListViewStateType.GetField(
                        "selectedItems",
                        BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
                }
                object state = TryGetListViewState(listView);
                if (state == null || _listViewSelectedItemsField == null)
                    return null;
                return _listViewSelectedItemsField.GetValue(state) as bool[];
            }
            catch
            {
                return null;
            }
        }

        private static int TryGetListViewFocusedRow(object listView)
        {
            try
            {
                if (_listViewRowField == null && ListViewStateType != null)
                {
                    _listViewRowField = ListViewStateType.GetField(
                        "row",
                        BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
                }
                object state = TryGetListViewState(listView);
                if (state == null || _listViewRowField == null)
                    return -1;
                object value = _listViewRowField.GetValue(state);
                if (value == null) return -1;
                return Convert.ToInt32(value);
            }
            catch
            {
                return -1;
            }
        }

        private static object TryGetListViewState(object listView)
        {
            if (listView == null) return null;
            Type listViewType = listView.GetType();
            FieldInfo stateField = listViewType.GetField(
                "m_State",
                BindingFlags.Instance | BindingFlags.NonPublic)
                ?? listViewType.GetField(
                    "state",
                    BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
            if (stateField == null)
                return null;
            try { return stateField.GetValue(listView); }
            catch { return null; }
        }

        private static object TryGetConsoleListView(EditorWindow consoleWindow)
        {
            try
            {
                if (_consoleWindowListViewField == null)
                {
                    _consoleWindowListViewField = ConsoleWindowType.GetField(
                        "m_ListView",
                        BindingFlags.Instance | BindingFlags.NonPublic);
                }
                if (_consoleWindowListViewField == null)
                    return null;
                return _consoleWindowListViewField.GetValue(consoleWindow);
            }
            catch
            {
                return null;
            }
        }

        private static EditorWindow FindOpenConsoleWindow()
        {
            try
            {
                UnityEngine.Object[] windows = Resources.FindObjectsOfTypeAll(ConsoleWindowType);
                if (windows == null || windows.Length == 0)
                    return null;
                return windows[0] as EditorWindow;
            }
            catch
            {
                return null;
            }
        }

        // ─── Reflection binding cache ───

        private static bool ResolveBindings()
        {
            if (_bindingsResolved) return true;
            if (_bindingFailed)
            {
                if (EditorApplication.timeSinceStartup < _nextBindingsRetryAt)
                    return false;
                _bindingFailed = false;
            }

            try
            {
                if (LogEntriesType != null)
                {
                    _logEntriesGetCount = LogEntriesType.GetMethod(
                        "GetCount",
                        BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic,
                        null, Type.EmptyTypes, null);
                    _logEntriesStartGettingEntries = LogEntriesType.GetMethod(
                        "StartGettingEntries",
                        BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic,
                        null, Type.EmptyTypes, null);
                    _logEntriesEndGettingEntries = LogEntriesType.GetMethod(
                        "EndGettingEntries",
                        BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic,
                        null, Type.EmptyTypes, null);
                    _logEntriesGetEntryInternal = LogEntriesType.GetMethod(
                        "GetEntryInternal",
                        BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic);
                    _logEntriesGetLinesAndMode = LogEntriesType.GetMethod(
                        "GetLinesAndModeFromEntryInternal",
                        BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic);
                }
                _bindingsResolved = _logEntriesGetCount != null
                    && _logEntriesGetEntryInternal != null;
                if (!_bindingsResolved)
                {
                    _bindingFailed = true;
                    _nextBindingsRetryAt = EditorApplication.timeSinceStartup + BindingsRetryIntervalSeconds;
                }
            }
            catch
            {
                _bindingFailed = true;
                _nextBindingsRetryAt = EditorApplication.timeSinceStartup + BindingsRetryIntervalSeconds;
            }
            return _bindingsResolved;
        }

        // ─── Reflection helpers ───

        private static Type FindEditorType(params string[] names)
        {
            Assembly editorAssembly = typeof(EditorWindow).Assembly;
            if (names == null) return null;
            foreach (string name in names)
            {
                if (string.IsNullOrEmpty(name)) continue;
                Type type = editorAssembly.GetType(name);
                if (type != null) return type;
            }
            return null;
        }
    }
}
