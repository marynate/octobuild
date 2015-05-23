using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
using WixSharp;

class Script
{
    static public string ReadVersion(string path)
    {
        System.IO.StreamReader file = new System.IO.StreamReader(path);
        Regex regex = new Regex("^\\s*version\\s*=\\s*\"(\\S+)\"");
        string line;
        while ((line = file.ReadLine()) != null)
        {
            Match match = regex.Match(line);
            if (match.Success)
            {
                return match.Groups[1].Value;
            }
        }
        return null;
    }

    static Platform ReadPlatform(string path)
    {
        String target = System.IO.File.ReadAllText(path);
        switch (target)
        {
            case "x86_64-pc-windows-gnu":
                return Platform.x64;
            case "i686-pc-windows-gnu":
                return Platform.x86;
            default:
                throw new Exception("Unknown target: " + target);
        }
    }

    static string PlatformName(Platform platform)
    {
        switch (platform)
        {
            case Platform.x64:
                return "x86_64";
            case Platform.x86:
                return "i686";
            default:
                throw new Exception("Unknown platform: " + platform);
        }
    }

    static void CreateNuspec(string template, string output, string version, Platform target)
    {
        string content = System.IO.File.ReadAllText(template, Encoding.UTF8);
        content = content.Replace("$version$", version);
        content = content.Replace("$target$", PlatformName(target));
        System.IO.File.WriteAllText(output, content, Encoding.UTF8);
    }

    static public void Main(string[] args)
    {
        Console.WriteLine("WixSharp version: " + FileVersionInfo.GetVersionInfo(typeof(WixSharp.Project).Assembly.Location).FileVersion);

        Platform platform = ReadPlatform(@"target\release\target.txt");
        String version = ReadVersion(@"Cargo.toml");
        Feature featureBuilder = new Feature("Octobuild Builder", true, false);
        featureBuilder.AttributesDefinition = @"AllowAdvertise=no";

        List<WixEntity> files = new List<WixEntity>();
        files.Add(new File(featureBuilder, @"target\release\xgconsole.exe"));
        files.Add(new File(featureBuilder, @"LICENSE"));
        foreach (string file in System.IO.Directory.GetFiles(@"target\release", "*.dll"))
        {
            files.Add(new File(featureBuilder, file));
        }

        List<WixEntity> projectEntries = new List<WixEntity>();
        projectEntries.AddRange(new WixEntity[] {
            new Property("ApplicationFolderName", "Octobuild"),
            new Property("WixAppFolder", "WixPerMachineFolder"),
            new Dir(new Id("APPLICATIONFOLDER"), @"%ProgramFiles%\Octobuild", files.ToArray()),
            new EnvironmentVariable(featureBuilder, "PATH", "[APPLICATIONFOLDER]")
            {
                Permanent = false,
                Part = EnvVarPart.last,
                Action = EnvVarAction.set,
                System = true,
                Condition = new Condition("ALLUSERS")
            },
            new EnvironmentVariable(featureBuilder, "PATH", "[APPLICATIONFOLDER]")
            {
                Permanent = false,
                Part = EnvVarPart.last,
                Action = EnvVarAction.set,
                System = false,
                Condition = new Condition("NOT ALLUSERS")
            }
        });

        // Workarong for bug with invalid default installation path "C:\Program Files (x86)" for x86_64 platform.
        if (platform == Platform.x64)
        {
            foreach (Sequence sequence in new Sequence[] { Sequence.InstallUISequence, Sequence.InstallExecuteSequence })
            {
                projectEntries.Add(
                    new SetPropertyAction("WixPerMachineFolder", "[ProgramFiles64Folder][ApplicationFolderName]")
                    {
                        Execute = Execute.immediate,
                        When = When.After,
                        Sequence = sequence,
                        Step = new Step("WixSetDefaultPerMachineFolder")
                    }
                );
            }
        }

        Project project = new Project("Octobuild", projectEntries.ToArray());
        project.ControlPanelInfo.Manufacturer = "Artem V. Navrotskiy";
        project.ControlPanelInfo.UrlInfoAbout = "https://github.com/bozaro/octobuild";
        project.LicenceFile = @"LICENSE.rtf";
        project.LicenceFile = @"LICENSE.rtf";
        project.GUID = new Guid("b4505233-6377-406b-955b-2547d86a99a7");
        project.UI = WUI.WixUI_Advanced;
        project.Version = new Version(version);
        project.OutFileName = @"target\octobuild-" + version + "-" + PlatformName(platform);
        project.Platform = Platform.x64;
        project.Package.AttributesDefinition = @"InstallPrivileges=elevated;InstallScope=perMachine";

        Compiler.BuildMsi(project);
        Compiler.BuildWxs(project);
        CreateNuspec(@"wixcs\octobuild.nuspec", @"target\octobuild.nuspec", version, platform);
    }
}
