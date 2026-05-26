/***************************************************************
 * @Author         : seem.sky@gmail.com
 * @Email          : seem.sky@gmail.com
 * @Description    :
 * @FilePath       : \locus_unity\Editor\LocusBridge.LuaGc.cs
 * @Date           : 2026-05-26 16:40:44
 * @LastEditTime   : 2026-05-26 16:52:30
 * @LastEditors    : seem.sky@gmail.com seem.sky@gmail.com
***************************************************************/

using UnityEngine;
using UnityEditor;

using System;
using System.IO;
using System.Globalization;
using System.Collections.Generic;
using System.Reflection;

namespace Locus
{
    /// <summary>
    /// Play Mode Lua / xLua GC sampling for Locus. Uses reflection when XLua is present;
    /// games can register a LuaEnv via <see cref="LuaGcBootstrap.Register"/>.
    /// </summary>
    public static partial class LocusBridge
    {
        private const string LuaGcRootFolder = "Library/Locus/LuaGc";
        private const int DefaultSampleIntervalMs = 100;
        private const int MinSampleIntervalMs = 50;
        private const int MaxSampleIntervalMs = 2000;

        private static readonly object LuaGcLock = new object();
        private static LuaGcMonitorSession _activeLuaGcSession;

        [Serializable]
        private sealed class LuaGcMonitorStartRequest
        {
            public string session_id;
            public int sample_interval_ms;
        }

        [Serializable]
        private sealed class LuaGcSamplePayload
        {
            public string sessionId;
            public int frame;
            public long timeMs;
            public string runtime;
            public double memoryKb;
            public double gcDebtKb;
            public int gcStepMult;
            public bool gcRunning;
            public string gcPhase;
            public double allocKbSinceLast;
            public string luaVersion;
            public bool runtimeAvailable;
            public string runtimeMessage;
        }

        [Serializable]
        private sealed class LuaGcMonitorStatusPayload
        {
            public bool active;
            public string sessionId;
            public int sampleIntervalMs;
            public int sampleCount;
            public bool runtimeAvailable;
            public string runtime;
            public string runtimeMessage;
        }

        private sealed class LuaGcMonitorSession
        {
            public readonly string SessionId;
            public readonly int SampleIntervalMs;
            public double LastSampleRealtime;
            public double LastMemoryKb;
            public int SampleCount;
            public string SessionDir;

            public LuaGcMonitorSession(string sessionId, int sampleIntervalMs)
            {
                SessionId = sessionId;
                SampleIntervalMs = sampleIntervalMs;
                LastSampleRealtime = -1.0;
                LastMemoryKb = double.NaN;
            }
        }

        /// <summary>
        /// Optional game-side hook to supply the active xLua (or other) Lua environment.
        /// </summary>
        public static class LuaGcBootstrap
        {
            private static Func<object> _envProvider;
            private static string _runtimeLabel = "xlua";

            public static void Register(Func<object> getLuaEnv, string runtimeLabel = "xlua")
            {
                _envProvider = getLuaEnv;
                _runtimeLabel = string.IsNullOrWhiteSpace(runtimeLabel) ? "xlua" : runtimeLabel.Trim();
            }

            public static void Unregister()
            {
                _envProvider = null;
                _runtimeLabel = "xlua";
            }

            internal static string RuntimeLabel => _runtimeLabel;

            internal static object ResolveEnv()
            {
                if (_envProvider != null)
                {
                    try
                    {
                        return _envProvider();
                    }
                    catch (Exception ex)
                    {
                        Debug.LogWarning("[Locus] LuaGcBootstrap provider failed: " + ex.Message);
                    }
                }

                return XLuaGcReflection.TryResolveDefaultEnv();
            }
        }

        private static class XLuaGcReflection
        {
            private static Type _luaEnvType;
            private static MethodInfo _doString;
            private static PropertyInfo _memoryProp;
            private static bool _typesResolved;
            private static bool _typesAvailable;

            private static void EnsureTypes()
            {
                if (_typesResolved)
                    return;
                _typesResolved = true;

                foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
                {
                    try
                    {
                        var type = assembly.GetType("XLua.LuaEnv", false);
                        if (type == null)
                            continue;
                        _luaEnvType = type;
                        _doString = FindDoStringMethod(type);

                        _memoryProp = type.GetProperty("Memroy", BindingFlags.Instance | BindingFlags.Public)
                            ?? type.GetProperty("Memory", BindingFlags.Instance | BindingFlags.Public);
                        break;
                    }
                    catch
                    {
                        // ignore broken assemblies
                    }
                }

                _typesAvailable = _luaEnvType != null && (_doString != null || _memoryProp != null);
            }

            internal static object TryResolveDefaultEnv()
            {
                EnsureTypes();
                if (!_typesAvailable || _luaEnvType == null)
                    return null;

                try
                {
                    foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
                    {
                        foreach (var type in SafeGetTypes(assembly))
                        {
                            if (type == null || type.IsAbstract || type.IsInterface)
                                continue;
                            var field = type.GetField(
                                "luaEnv",
                                BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic
                            );
                            if (field != null && _luaEnvType.IsAssignableFrom(field.FieldType))
                            {
                                var value = field.GetValue(null);
                                if (value != null)
                                    return value;
                            }

                            var prop = type.GetProperty(
                                "LuaEnv",
                                BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic
                            );
                            if (prop != null && _luaEnvType.IsAssignableFrom(prop.PropertyType))
                            {
                                var value = prop.GetValue(null, null);
                                if (value != null)
                                    return value;
                            }
                        }
                    }
                }
                catch
                {
                    // best effort
                }

                return null;
            }

            private static MethodInfo FindDoStringMethod(Type luaEnvType)
            {
                foreach (var method in luaEnvType.GetMethods(BindingFlags.Instance | BindingFlags.Public))
                {
                    if (!string.Equals(method.Name, "DoString", StringComparison.Ordinal))
                        continue;
                    var parameters = method.GetParameters();
                    if (parameters.Length == 0)
                        continue;
                    if (parameters[0].ParameterType != typeof(string))
                        continue;
                    return method;
                }

                return null;
            }

            private static IEnumerable<Type> SafeGetTypes(Assembly assembly)
            {
                try
                {
                    return assembly.GetTypes();
                }
                catch (ReflectionTypeLoadException ex)
                {
                    return ex.Types ?? Array.Empty<Type>();
                }
            }

            internal static bool TrySample(object env, out double memoryKb, out string luaVersion, out string message)
            {
                memoryKb = 0.0;
                luaVersion = "unknown";
                message = "";

                EnsureTypes();
                if (env == null)
                {
                    message = "No Lua environment. Call Locus.LuaGcBootstrap.Register(() => yourLuaEnv).";
                    return false;
                }

                if (!_typesAvailable || !_luaEnvType.IsInstanceOfType(env))
                {
                    message = "Registered object is not XLua.LuaEnv.";
                    return false;
                }

                try
                {
                    if (_memoryProp != null)
                    {
                        var mem = _memoryProp.GetValue(env, null);
                        if (mem is int intMem)
                            memoryKb = intMem;
                        else if (mem is long longMem)
                            memoryKb = longMem;
                        else if (mem != null)
                            memoryKb = Convert.ToDouble(mem, CultureInfo.InvariantCulture);
                    }

                    if (_doString != null && (memoryKb <= 0.0 || double.IsNaN(memoryKb)))
                    {
                        var script =
                            "local c = collectgarbage('count'); "
                            + "local v = _VERSION or 'unknown'; "
                            + "return c, v";
                        object result = InvokeDoString(env, script);
                        if (result is object[] arr && arr.Length > 0)
                        {
                            memoryKb = Convert.ToDouble(arr[0], CultureInfo.InvariantCulture);
                            if (arr.Length > 1 && arr[1] != null)
                                luaVersion = arr[1].ToString();
                        }
                        else if (result != null)
                        {
                            memoryKb = Convert.ToDouble(result, CultureInfo.InvariantCulture);
                        }
                    }
                    else if (_doString != null)
                    {
                        object result = InvokeDoString(env, "return _VERSION or 'unknown'");
                        if (result is object[] arr && arr.Length > 0 && arr[0] != null)
                            luaVersion = arr[0].ToString();
                        else if (result != null)
                            luaVersion = result.ToString();
                    }

                    return memoryKb >= 0.0 && !double.IsNaN(memoryKb);
                }
                catch (Exception ex)
                {
                    message = ex.Message;
                    return false;
                }
            }

            private static object InvokeDoString(object env, string script)
            {
                var parameters = _doString.GetParameters();
                if (parameters.Length == 1)
                    return _doString.Invoke(env, new object[] { script });
                if (parameters.Length == 2)
                    return _doString.Invoke(env, new object[] { script, "LocusLuaGc" });
                return _doString.Invoke(env, new object[] { script, "LocusLuaGc", null });
            }
        }

        private static PipeEnvelope HandleLuaGcMonitorStart(string reqId, string message)
        {
            LuaGcMonitorStartRequest request;
            try
            {
                request = string.IsNullOrWhiteSpace(message)
                    ? new LuaGcMonitorStartRequest()
                    : JsonUtility.FromJson<LuaGcMonitorStartRequest>(message);
            }
            catch (Exception ex)
            {
                return ErrorResponse(reqId, "Invalid lua_gc_monitor_start payload: " + ex.Message);
            }

            if (request == null)
                request = new LuaGcMonitorStartRequest();

            if (!EditorApplication.isPlaying)
            {
                return ErrorResponse(reqId, "lua_gc_monitor requires Play Mode.");
            }

            string sessionId = string.IsNullOrWhiteSpace(request.session_id)
                ? Guid.NewGuid().ToString("N")
                : request.session_id.Trim();

            int interval = request.sample_interval_ms > 0
                ? request.sample_interval_ms
                : DefaultSampleIntervalMs;
            interval = Mathf.Clamp(interval, MinSampleIntervalMs, MaxSampleIntervalMs);

            lock (LuaGcLock)
            {
                StopLuaGcMonitorInternal("replaced");
                var session = new LuaGcMonitorSession(sessionId, interval);
                session.SessionDir = EnsureLuaGcSessionDir(sessionId);
                WriteLuaGcMeta(session);
                _activeLuaGcSession = session;
            }

            EditorApplication.update -= OnLuaGcMonitorUpdate;
            EditorApplication.update += OnLuaGcMonitorUpdate;
            EditorApplication.playModeStateChanged -= OnLuaGcPlayModeStateChanged;
            EditorApplication.playModeStateChanged += OnLuaGcPlayModeStateChanged;

            var status = BuildLuaGcStatus();
            return OkResponse(reqId, JsonUtility.ToJson(status));
        }

        private static PipeEnvelope HandleLuaGcMonitorStop(string reqId, string message)
        {
            lock (LuaGcLock)
            {
                StopLuaGcMonitorInternal(string.IsNullOrWhiteSpace(message) ? "stopped" : message.Trim());
            }

            return OkResponse(reqId, JsonUtility.ToJson(BuildLuaGcStatus()));
        }

        private static PipeEnvelope HandleLuaGcMonitorStatus(string reqId)
        {
            return OkResponse(reqId, JsonUtility.ToJson(BuildLuaGcStatus()));
        }

        private static void OnLuaGcPlayModeStateChanged(PlayModeStateChange state)
        {
            if (state == PlayModeStateChange.EnteredEditMode)
            {
                lock (LuaGcLock)
                {
                    StopLuaGcMonitorInternal("play_mode_ended");
                }
            }
        }

        private static void OnLuaGcMonitorUpdate()
        {
            LuaGcMonitorSession session;
            lock (LuaGcLock)
            {
                session = _activeLuaGcSession;
            }

            if (session == null)
                return;

            if (!EditorApplication.isPlaying)
            {
                lock (LuaGcLock)
                {
                    StopLuaGcMonitorInternal("play_mode_ended");
                }
                return;
            }

            double now = EditorApplication.timeSinceStartup;
            if (session.LastSampleRealtime >= 0.0)
            {
                double elapsedMs = (now - session.LastSampleRealtime) * 1000.0;
                if (elapsedMs < session.SampleIntervalMs)
                    return;
            }

            session.LastSampleRealtime = now;
            EmitLuaGcSample(session);
        }

        private static void EmitLuaGcSample(LuaGcMonitorSession session)
        {
            object env = LuaGcBootstrap.ResolveEnv();
            bool available = XLuaGcReflection.TrySample(
                env,
                out double memoryKb,
                out string luaVersion,
                out string runtimeMessage
            );

            double allocKb = 0.0;
            if (!double.IsNaN(session.LastMemoryKb) && available)
                allocKb = Math.Max(0.0, memoryKb - session.LastMemoryKb);
            if (available)
                session.LastMemoryKb = memoryKb;

            session.SampleCount += 1;

            var payload = new LuaGcSamplePayload
            {
                sessionId = session.SessionId,
                frame = Time.frameCount,
                timeMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                runtime = LuaGcBootstrap.RuntimeLabel,
                memoryKb = memoryKb,
                gcDebtKb = 0.0,
                gcStepMult = 0,
                gcRunning = false,
                gcPhase = available ? "incremental" : "unknown",
                allocKbSinceLast = allocKb,
                luaVersion = luaVersion,
                runtimeAvailable = available,
                runtimeMessage = runtimeMessage ?? ""
            };

            string json = JsonUtility.ToJson(payload);
            SendEventToRust("lua-gc-sample", json);
            AppendLuaGcSampleLine(session, json);
        }

        private static void StopLuaGcMonitorInternal(string reason)
        {
            if (_activeLuaGcSession == null)
                return;

            EditorApplication.update -= OnLuaGcMonitorUpdate;
            EditorApplication.playModeStateChanged -= OnLuaGcPlayModeStateChanged;

            string sessionId = _activeLuaGcSession.SessionId;
            _activeLuaGcSession = null;

            var stopped = new
            {
                sessionId = sessionId,
                reason = reason ?? "stopped"
            };
            SendEventToRust("lua-gc-monitor-stopped", JsonUtility.ToJson(stopped));
        }

        private static LuaGcMonitorStatusPayload BuildLuaGcStatus()
        {
            lock (LuaGcLock)
            {
                if (_activeLuaGcSession == null)
                {
                    object env = LuaGcBootstrap.ResolveEnv();
                    string idleMessage = "";
                    bool available = env != null
                        && XLuaGcReflection.TrySample(env, out _, out _, out idleMessage);
                    return new LuaGcMonitorStatusPayload
                    {
                        active = false,
                        sessionId = "",
                        sampleIntervalMs = DefaultSampleIntervalMs,
                        sampleCount = 0,
                        runtimeAvailable = available,
                        runtime = LuaGcBootstrap.RuntimeLabel,
                        runtimeMessage = available
                            ? ""
                            : (string.IsNullOrEmpty(idleMessage)
                                ? "Lua runtime not registered."
                                : idleMessage)
                    };
                }

                string runtimeMessage = "";
                object activeEnv = LuaGcBootstrap.ResolveEnv();
                bool runtimeAvailable = activeEnv != null
                    && XLuaGcReflection.TrySample(activeEnv, out _, out _, out runtimeMessage);
                return new LuaGcMonitorStatusPayload
                {
                    active = true,
                    sessionId = _activeLuaGcSession.SessionId,
                    sampleIntervalMs = _activeLuaGcSession.SampleIntervalMs,
                    sampleCount = _activeLuaGcSession.SampleCount,
                    runtimeAvailable = runtimeAvailable,
                    runtime = LuaGcBootstrap.RuntimeLabel,
                    runtimeMessage = runtimeMessage ?? ""
                };
            }
        }

        private static string EnsureLuaGcSessionDir(string sessionId)
        {
            string projectRoot = Directory.GetParent(Application.dataPath)?.FullName ?? "";
            string dir = Path.Combine(projectRoot, LuaGcRootFolder, sessionId);
            Directory.CreateDirectory(dir);
            return dir;
        }

        private static void WriteLuaGcMeta(LuaGcMonitorSession session)
        {
            if (string.IsNullOrEmpty(session.SessionDir))
                return;

            string metaPath = Path.Combine(session.SessionDir, "meta.json");
            string json =
                "{"
                + "\"sessionId\":\""
                + EscapeJson(session.SessionId)
                + "\","
                + "\"sampleIntervalMs\":"
                + session.SampleIntervalMs.ToString(CultureInfo.InvariantCulture)
                + ","
                + "\"startedAtMs\":"
                + DateTimeOffset.UtcNow.ToUnixTimeMilliseconds().ToString(CultureInfo.InvariantCulture)
                + "}";
            File.WriteAllText(metaPath, json);
        }

        private static void AppendLuaGcSampleLine(LuaGcMonitorSession session, string json)
        {
            if (string.IsNullOrEmpty(session.SessionDir))
                return;

            try
            {
                string samplesPath = Path.Combine(session.SessionDir, "samples.ndjson");
                File.AppendAllText(samplesPath, json + "\n");
            }
            catch (Exception ex)
            {
                Debug.LogWarning("[Locus] Failed to append lua gc sample: " + ex.Message);
            }
        }

        private static string EscapeJson(string value)
        {
            return (value ?? "").Replace("\\", "\\\\").Replace("\"", "\\\"");
        }
    }
}
