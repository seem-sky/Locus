// A floating utility EditorWindow that tracks the Unity Console window and renders
// a "Send to Locus" mini-toolbar above (or alongside) it. Avoids touching the
// internal UnityEditor.ConsoleWindow API and works across Unity 2020+.

using UnityEditor;
using UnityEngine;

using System;
using System.Reflection;

namespace Locus
{
    internal sealed class LocusConsoleToolbarWindow : EditorWindow
    {
        private const float WindowWidth = 168f;
        private const float WindowHeight = 22f;
        private const float ConsoleBottomOffset = 6f;
        private const double RepositionIntervalSeconds = 0.2d;

        private static readonly Type ConsoleWindowType =
            FindEditorType("UnityEditor.ConsoleWindow", "UnityEditorInternal.ConsoleWindow");

        private double _nextRepositionAt;
        private EditorWindow _trackedConsoleWindow;
        private bool _consoleWasVisible;

        public static LocusConsoleToolbarWindow ShowToolbar()
        {
            LocusConsoleToolbarWindow window = CreateInstance<LocusConsoleToolbarWindow>();
            window.titleContent = new GUIContent("Locus Console Toolbar");
            window.minSize = new Vector2(WindowWidth, WindowHeight);
            window.maxSize = new Vector2(WindowWidth, WindowHeight);
            window.ShowUtility();
            window._trackedConsoleWindow = FindOpenConsoleWindow();
            window._consoleWasVisible = window._trackedConsoleWindow != null;
            window.RepositionToConsole(true);
            return window;
        }

        public static void HideToolbar()
        {
            LocusConsoleToolbarWindow[] windows = Resources.FindObjectsOfTypeAll<LocusConsoleToolbarWindow>();
            for (int i = 0; i < windows.Length; i++)
            {
                if (windows[i] != null)
                    windows[i].Close();
            }
        }

        public static bool IsToolbarOpen()
        {
            return Resources.FindObjectsOfTypeAll<LocusConsoleToolbarWindow>().Length > 0;
        }

        private void OnEnable()
        {
            EditorApplication.update += PumpReposition;
        }

        private void OnDisable()
        {
            EditorApplication.update -= PumpReposition;
        }

        private void OnGUI()
        {
            DrawToolbar();
        }

        private void DrawToolbar()
        {
            using (new EditorGUILayout.HorizontalScope(EditorStyles.toolbar))
            {
                if (GUILayout.Button(new GUIContent("Send Selected", "Send the selected Unity Console entries to Locus (falls back to full console if nothing is selected)."), EditorStyles.toolbarButton, GUILayout.Width(96f)))
                {
                    LocusConsoleIntegration.SendSelectedToLocus();
                    GUIUtility.ExitGUI();
                }

                using (new EditorGUI.DisabledScope(_trackedConsoleWindow == null))
                {
                    if (GUILayout.Button(new GUIContent("Send All", "Send the trailing Unity Console entries to Locus."), EditorStyles.toolbarButton, GUILayout.Width(56f)))
                    {
                        LocusConsoleIntegration.SendAllToLocus();
                        GUIUtility.ExitGUI();
                    }
                }
            }
        }

        private void PumpReposition()
        {
            double now = EditorApplication.timeSinceStartup;
            if (now < _nextRepositionAt)
                return;
            _nextRepositionAt = now + RepositionIntervalSeconds;

            _trackedConsoleWindow = FindOpenConsoleWindow();
            bool consoleVisible = _trackedConsoleWindow != null;
            if (consoleVisible != _consoleWasVisible)
            {
                _consoleWasVisible = consoleVisible;
                Repaint();
            }
            RepositionToConsole(false);
        }

        private void RepositionToConsole(bool force)
        {
            if (_trackedConsoleWindow == null)
            {
                if (!force)
                    return;
                // Park the toolbar in the top-right of the main editor when no console is open yet.
                Vector2 mainSize = new Vector2(Screen.width, Screen.height);
                if (mainSize.x <= 0 || mainSize.y <= 0)
                    return;
                position = new Rect(
                    mainSize.x - WindowWidth - 16f,
                    48f,
                    WindowWidth,
                    WindowHeight);
                return;
            }

            Rect consoleRect = _trackedConsoleWindow.position;
            if (consoleRect.width <= 0 || consoleRect.height <= 0)
                return;

            // Anchor the toolbar just above the console window so it reads as a Console toolbar.
            float x = consoleRect.x + 6f;
            float y = consoleRect.y - WindowHeight - ConsoleBottomOffset;
            if (y < 4f)
            {
                // No room above the console (docked at top). Fall back to a strip glued
                // to the top of the console content area.
                y = consoleRect.y + 4f;
            }

            Rect target = new Rect(x, y, WindowWidth, WindowHeight);
            if (!force && target.Approximately(position))
                return;
            position = target;
        }

        private static EditorWindow FindOpenConsoleWindow()
        {
            if (ConsoleWindowType == null)
                return null;
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

    internal static class RectExtensions
    {
        public static bool Approximately(this Rect a, Rect b)
        {
            const float epsilon = 0.5f;
            return Mathf.Abs(a.x - b.x) < epsilon
                && Mathf.Abs(a.y - b.y) < epsilon
                && Mathf.Abs(a.width - b.width) < epsilon
                && Mathf.Abs(a.height - b.height) < epsilon;
        }
    }
}
