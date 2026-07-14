using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Windows;
using Microsoft.Win32;
using Microsoft.Tools.WindowsInstallerXml.Bootstrapper;

[assembly: BootstrapperApplication(typeof(CodeStudioLite.Installer.CodeStudioBootstrapperApplication))]

namespace CodeStudioLite.Installer
{
    internal enum InstallerPage
    {
        Welcome,
        Options,
        Confirm,
        Progress,
        Complete,
    }

    internal sealed class InstallerLanguage
    {
        public InstallerLanguage(string code, string label)
        {
            Code = code;
            Label = label;
        }

        public string Code { get; }
        public string Label { get; }
        public override string ToString() => Label;
    }

    public sealed class CodeStudioBootstrapperApplication : BootstrapperApplication
    {
        private InstallerWindow form;
        private LaunchAction plannedAction;
        private string selectedLanguage = "en-US";
        private string installFolder;
        private bool applying;
        private bool hasFolderOverride;
        private bool verifyPlanOnly;
        private bool launchAfterInstall;
        private readonly HashSet<string> sameVersionRelatedBundles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        protected override void Run()
        {
            string configuredLanguage = CommandLineValue("SelectedLanguage");
            string configuredFolder = CommandLineValue("InstallFolder") ?? Engine.StringVariables["InstallFolder"];
            hasFolderOverride = !string.IsNullOrWhiteSpace(configuredFolder);
            verifyPlanOnly = CommandLineValue("VerifyPlanOnly") == "1" || Engine.NumericVariables["VerifyPlanOnly"] == 1;
            launchAfterInstall = CommandLineValue("LaunchAfterInstall") == "1";

            selectedLanguage = IsSupportedLanguage(configuredLanguage) ? configuredLanguage : DefaultLanguageCode();
            string discoveredFolder = hasFolderOverride ? null : RegistryInstallLocation(null);
            installFolder = NormalizeInstallFolder(hasFolderOverride
                ? configuredFolder
                : (!string.IsNullOrWhiteSpace(discoveredFolder) ? discoveredFolder : DefaultInstallFolder()));
            Engine.StringVariables["SelectedLanguage"] = selectedLanguage;
            Engine.StringVariables["InstallFolder"] = installFolder;

            var application = new Application { ShutdownMode = ShutdownMode.OnMainWindowClose };
            form = new InstallerWindow(this, selectedLanguage, installFolder, Command.Action, Command.Display == Display.Full);
            DetectRelatedBundle += OnDetectRelatedBundle;
            DetectRelatedMsiPackage += OnDetectRelatedMsiPackage;
            DetectComplete += OnDetectComplete;
            PlanPackageBegin += OnPlanPackageBegin;
            PlanRelatedBundle += OnPlanRelatedBundle;
            PlanComplete += OnPlanComplete;
            Progress += OnProgress;
            ApplyComplete += OnApplyComplete;

            form.Loaded += (_, __) => Engine.Detect();
            application.Run(form);
            Engine.Quit(form.ExitCode);
        }

        internal void BeginAction(string languageCode, string installFolder, LaunchAction action)
        {
            if (applying)
            {
                return;
            }

            string normalizedFolder;
            try
            {
                normalizedFolder = NormalizeInstallFolder(installFolder);
            }
            catch (ArgumentException)
            {
                form.ShowInvalidInstallFolder();
                return;
            }

            selectedLanguage = IsSupportedLanguage(languageCode) ? languageCode : "en-US";
            this.installFolder = normalizedFolder;
            Engine.StringVariables["SelectedLanguage"] = selectedLanguage;
            Engine.StringVariables["InstallFolder"] = this.installFolder;
            plannedAction = NormalizeAction(action);
            form.ShowPlanning(plannedAction);
            Engine.Plan(plannedAction);
        }

        internal void Cancel()
        {
            if (!applying)
            {
                form.ExitCode = 1602;
                form.Close();
            }
        }

        private void OnDetectRelatedBundle(object sender, DetectRelatedBundleEventArgs e)
        {
            if (e.RelationType == RelationType.Upgrade && e.Operation == RelatedOperation.None)
            {
                sameVersionRelatedBundles.Add(e.ProductCode);
            }
        }

        private void OnDetectRelatedMsiPackage(object sender, DetectRelatedMsiPackageEventArgs e)
        {
            if (e.PackageId != "MsiBase")
            {
                return;
            }

            if (!hasFolderOverride)
            {
                string installedFolder = InstalledProductLocation(e.ProductCode);
                if (!string.IsNullOrWhiteSpace(installedFolder))
                {
                    installFolder = NormalizeInstallFolder(installedFolder);
                    Engine.StringVariables["InstallFolder"] = installFolder;
                    form.SetInstallFolder(installFolder);
                }
            }
        }

        private void OnDetectComplete(object sender, DetectCompleteEventArgs e)
        {
            if (e.Status < 0)
            {
                form.ShowFailure(e.Status, Localized(selectedLanguage, "Detection failed.", "检测失败。", "偵測失敗。"));
                CloseIfUnattended();
                return;
            }

            if (Command.Display != Display.Full)
            {
                BeginAction(selectedLanguage, installFolder, Command.Action);
                return;
            }

            form.ShowWelcome();
        }

        private void OnPlanPackageBegin(object sender, PlanPackageBeginEventArgs e)
        {
            if (e.PackageId != "MsiBase" || plannedAction == LaunchAction.Layout || plannedAction == LaunchAction.Cache)
            {
                return;
            }

            if (plannedAction == LaunchAction.Uninstall)
            {
                e.State = RequestState.Absent;
                return;
            }

            e.State = plannedAction == LaunchAction.Repair ? RequestState.Repair : RequestState.Present;
        }

        private void OnPlanRelatedBundle(object sender, PlanRelatedBundleEventArgs e)
        {
            if (plannedAction == LaunchAction.Install && sameVersionRelatedBundles.Contains(e.BundleId))
            {
                e.State = RequestState.Absent;
            }
        }

        private void OnPlanComplete(object sender, PlanCompleteEventArgs e)
        {
            if (e.Status < 0)
            {
                form.ShowFailure(e.Status, Localized(selectedLanguage, "Planning failed.", "安装规划失败。", "安裝規劃失敗。"));
                CloseIfUnattended();
                return;
            }

            if (verifyPlanOnly)
            {
                form.CloseAfterPlanVerification();
                return;
            }

            IntPtr applyWindowHandle = form.Handle;
            if (applyWindowHandle == IntPtr.Zero)
            {
                form.ShowFailure(unchecked((int)0x80004005), Localized(selectedLanguage, "Setup window initialization failed.", "安装窗口初始化失败。", "安裝視窗初始化失敗。"));
                CloseIfUnattended();
                return;
            }

            applying = true;
            form.ShowApplying();
            Engine.Apply(applyWindowHandle);
        }

        private void OnProgress(object sender, ProgressEventArgs e) => form.SetProgress(e.OverallPercentage);

        private void OnApplyComplete(object sender, ApplyCompleteEventArgs e)
        {
            applying = false;
            form.ExitCode = e.Status;
            form.ShowComplete(e.Status, e.Restart);
            if (Command.Display != Display.Full)
            {
                if (e.Status >= 0 && e.Restart == ApplyRestart.None && launchAfterInstall &&
                    (plannedAction == LaunchAction.Install || plannedAction == LaunchAction.Repair))
                {
                    try
                    {
                        LaunchInstalledApp();
                    }
                    catch (Exception error)
                    {
                        Engine.Log(LogLevel.Error, "CodeStudio Lite was updated but could not be restarted: " + error.Message);
                    }
                }
                form.CloseOnUiThread();
            }
        }

        internal void LaunchInstalledApp()
        {
            string executablePath = Path.Combine(installFolder, "codestudio-lite.exe");
            if (!File.Exists(executablePath))
            {
                throw new FileNotFoundException("The installed application executable was not found.", executablePath);
            }

            Process.Start(new ProcessStartInfo
            {
                FileName = executablePath,
                WorkingDirectory = installFolder,
                UseShellExecute = true,
            });
        }

        private void CloseIfUnattended()
        {
            if (Command.Display != Display.Full)
            {
                form.CloseOnUiThread();
            }
        }

        private static LaunchAction NormalizeAction(LaunchAction action)
        {
            switch (action)
            {
                case LaunchAction.Uninstall:
                case LaunchAction.Repair:
                case LaunchAction.Layout:
                case LaunchAction.Cache:
                    return action;
                default:
                    return LaunchAction.Install;
            }
        }

        private static bool IsSupportedLanguage(string languageCode) =>
            languageCode == "zh-CN" || languageCode == "zh-TW" || languageCode == "en-US";

        private string CommandLineValue(string name)
        {
            string prefix = name + "=";
            foreach (string argument in Command.GetCommandLineArgs())
            {
                string normalized = argument.TrimStart('-', '/');
                if (normalized.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                {
                    return normalized.Substring(prefix.Length).Trim('"');
                }
            }
            return null;
        }

        private static string InstalledProductLocation(string productCode)
        {
            string location = InstalledProductValue(productCode, "InstallLocation");
            return !string.IsNullOrWhiteSpace(location) ? location : RegistryInstallLocation(productCode);
        }

        private static string RegistryInstallLocation(string productCode)
        {
            const string uninstallPath = @"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
            RegistryHive[] hives = { RegistryHive.LocalMachine, RegistryHive.CurrentUser };
            RegistryView[] views = { RegistryView.Registry64, RegistryView.Registry32 };

            foreach (RegistryHive hive in hives)
            {
                foreach (RegistryView view in views)
                {
                    try
                    {
                        using (RegistryKey baseKey = RegistryKey.OpenBaseKey(hive, view))
                        using (RegistryKey uninstallKey = baseKey.OpenSubKey(uninstallPath))
                        {
                            if (uninstallKey == null)
                            {
                                continue;
                            }

                            if (!string.IsNullOrWhiteSpace(productCode))
                            {
                                using (RegistryKey productKey = uninstallKey.OpenSubKey(productCode))
                                {
                                    string exactLocation = RegistryValue(productKey, "InstallLocation");
                                    if (!string.IsNullOrWhiteSpace(exactLocation))
                                    {
                                        return exactLocation;
                                    }
                                }
                            }

                            foreach (string subKeyName in uninstallKey.GetSubKeyNames())
                            {
                                using (RegistryKey productKey = uninstallKey.OpenSubKey(subKeyName))
                                {
                                    string displayName = RegistryValue(productKey, "DisplayName");
                                    if (!string.Equals(displayName, "CodeStudio Lite", StringComparison.OrdinalIgnoreCase))
                                    {
                                        continue;
                                    }

                                    string location = RegistryValue(productKey, "InstallLocation");
                                    if (!string.IsNullOrWhiteSpace(location))
                                    {
                                        return location;
                                    }
                                }
                            }
                        }
                    }
                    catch (UnauthorizedAccessException)
                    {
                        // Continue with the remaining registry scopes.
                    }
                }
            }

            return null;
        }

        private static string RegistryValue(RegistryKey key, string name) => key == null ? null : key.GetValue(name) as string;

        private static string InstalledProductValue(string productCode, string property)
        {
            uint length = 0;
            MsiGetProductInfo(productCode, property, null, ref length);
            if (length == 0)
            {
                return null;
            }

            length++;
            var value = new StringBuilder((int)length);
            return MsiGetProductInfo(productCode, property, value, ref length) == 0 ? value.ToString() : null;
        }

        [DllImport("msi.dll", CharSet = CharSet.Unicode)]
        private static extern uint MsiGetProductInfo(string product, string property, StringBuilder valueBuffer, ref uint valueBufferLength);

        [DllImport("kernel32.dll")]
        private static extern ushort GetUserDefaultUILanguage();

        [DllImport("kernel32.dll")]
        private static extern ushort GetSystemDefaultUILanguage();

        private static string DefaultInstallFolder()
        {
            string programFiles = Environment.GetEnvironmentVariable("ProgramW6432");
            if (string.IsNullOrWhiteSpace(programFiles))
            {
                programFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            }
            return Path.Combine(programFiles, "CodeStudio Lite");
        }

        private static string NormalizeInstallFolder(string folder)
        {
            if (string.IsNullOrWhiteSpace(folder) || !Path.IsPathRooted(folder))
            {
                throw new ArgumentException("The installation folder must be an absolute path.");
            }
            return Path.GetFullPath(folder.Trim().Trim('"')).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }

        private static string DefaultLanguageCode()
        {
            string name = CultureName(GetUserDefaultUILanguage());
            if (string.IsNullOrWhiteSpace(name))
            {
                name = CultureName(GetSystemDefaultUILanguage());
            }
            if (string.IsNullOrWhiteSpace(name))
            {
                name = CultureInfo.CurrentUICulture.Name;
            }

            if (name.Equals("zh-TW", StringComparison.OrdinalIgnoreCase) ||
                name.Equals("zh-HK", StringComparison.OrdinalIgnoreCase) ||
                name.Equals("zh-MO", StringComparison.OrdinalIgnoreCase))
            {
                return "zh-TW";
            }
            return name.StartsWith("zh", StringComparison.OrdinalIgnoreCase) ? "zh-CN" : "en-US";
        }

        private static string CultureName(ushort languageId)
        {
            try
            {
                return CultureInfo.GetCultureInfo(languageId).Name;
            }
            catch (CultureNotFoundException)
            {
                return null;
            }
        }

        internal static string Localized(string language, string english, string simplifiedChinese, string traditionalChinese)
        {
            return language == "zh-CN" ? simplifiedChinese : language == "zh-TW" ? traditionalChinese : english;
        }
    }
}
