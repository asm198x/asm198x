using System.Reflection;
using System.Runtime.Loader;
using System.Security.Cryptography;

// Version-pinned adapter: execute CSpect's code, never reimplement its parser.
try {
    if (args.Length != 2) throw new Exception("usage: Consumer <CSpect.exe> <fixture.map>");
    string executable = Path.GetFullPath(args[0]);
    string digest = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(executable))).ToLowerInvariant();
    if (digest != "0fa35824c6170eeffa723797a9451a0c0d558f315b5412f080e9ee6791f9a6f0")
        throw new Exception("Unrecognised CSpect binary: inspect its adapter before running");
    AssemblyLoadContext.Default.Resolving += (context, name) => {
        string file = Path.Combine(Path.GetDirectoryName(executable)!, name.Name + ".dll");
        return File.Exists(file) ? context.LoadFromAssemblyPath(file) : null;
    };
    var assembly = AssemblyLoadContext.Default.LoadFromAssemblyPath(executable);
    var module = assembly.ManifestModule;
    var symbolsType = assembly.GetType("A.z", true)!;
    var globals = assembly.GetType("a.E", true)!;
    var table = Activator.CreateInstance(symbolsType, true)!;
    const BindingFlags fields = BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static;
    globals.GetFields(fields).Single(f => f.FieldType == symbolsType).SetValue(null, table);
    globals.GetFields(fields).Single(f => f.Name == "E" && f.FieldType == typeof(string))
        .SetValue(null, Path.GetFullPath(args[1]));
    // Actual file-loading entry point, SNASM/CSPECTMAP mode (not Z88DK).
    ((MethodInfo)module.ResolveMethod(0x06000199)!).Invoke(null, new object[] { false });
    var byName = (MethodInfo)module.ResolveMethod(0x06000187)!;
    var byPhysical = (MethodInfo)module.ResolveMethod(0x0600018b)!;
    foreach (var (name, logical, physical, kind) in new[] {
        ("DRAW", 0xc010, 0x4010, 0),
        ("MUSIC", 0xc010, 0xc010, 0),
        ("ANSWER", 0xc010, 0x0010, 1),
    }) {
        var symbol = byName.Invoke(table, new object[] { name })
            ?? throw new Exception($"Missing symbol {name}");
        int actualLogical = (int)module.ResolveField(0x04000206)!.GetValue(symbol)!;
        int actualPhysical = (int)module.ResolveField(0x04000207)!.GetValue(symbol)!;
        int actualKind = Convert.ToInt32(module.ResolveField(0x04000204)!.GetValue(symbol));
        if ((actualLogical, actualPhysical, actualKind) != (logical, physical, kind))
            throw new Exception($"{name}: got {actualLogical:x4}/{actualPhysical:x6}/{actualKind}, expected {logical:x4}/{physical:x6}/{kind}");
        if (kind == 0 && (string?)byPhysical.Invoke(table, new object[] { physical, false }) != name)
            throw new Exception($"Physical lookup failed for {name}");
        Console.WriteLine($"PASS {name}: logical={logical:x4} physical={physical:x6} kind={kind}");
    }
    if (byName.Invoke(table, new object[] { "NOT_PRESENT" }) != null)
        throw new Exception("Missing-name lookup unexpectedly succeeded");
    Console.WriteLine("PASS CSpect 3.1.0.0 native map loader, symbol records and physical lookup");
    return 0;
} catch (Exception e) {
    Console.Error.WriteLine("FAIL " + (e.InnerException ?? e).Message);
    return 1;
}
