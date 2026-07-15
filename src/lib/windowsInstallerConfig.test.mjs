import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const config = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
const packageJson = JSON.parse(fs.readFileSync("package.json", "utf8"));

const burnDirectory = "installer/windows/burn";
const bundleSourcePath = `${burnDirectory}/bundle.wxs`;
const bootstrapperSourcePath = `${burnDirectory}/BootstrapperApplication.cs`;
const bootstrapperProjectPath = `${burnDirectory}/CodeStudioBootstrapper.csproj`;
const installerWindowXamlPath = `${burnDirectory}/InstallerWindow.xaml`;
const installerWindowSourcePath = `${burnDirectory}/InstallerWindow.xaml.cs`;
const buildScriptPath = `${burnDirectory}/build-burn.ps1`;
const verifyScriptPath = `${burnDirectory}/verify-burn.ps1`;

test("Windows installers keep stable upgrade identity and reject downgrades", () => {
  assert.equal(config.identifier, "com.codestudio.lite");
  assert.equal(config.bundle.windows.allowDowngrades, false);
  assert.equal(config.bundle.windows.wix.upgradeCode, "83dcf1cf-93d9-57d3-b567-bf98f108a380");
  assert.deepEqual(config.bundle.windows.wix.language, ["en-US"]);
  assert.equal(config.bundle.windows.nsis.installMode, "currentUser");
});

test("NSIS uses branded icons and a Chinese-English language selector", () => {
  assert.equal(config.bundle.windows.nsis.installerIcon, "icons/icon.ico");
  assert.equal(config.bundle.windows.nsis.uninstallerIcon, "icons/icon.ico");
  assert.ok(fs.statSync("src-tauri/icons/icon.ico").size > 0);
  assert.deepEqual(config.bundle.windows.nsis.languages, ["SimpChinese", "TradChinese", "English"]);
  assert.equal(config.bundle.windows.nsis.displayLanguageSelector, true);
  assert.equal(config.bundle.windows.nsis.startMenuFolder, "CodeStudio Lite");
});

test("Burn bundle embeds one base MSI without language transforms", () => {
  const source = fs.readFileSync(bundleSourcePath, "utf8");

  assert.match(source, /UpgradeCode="\{6B78C4D8-8C90-4C11-A1D8-893160DA17A7\}"/);
  assert.match(source, /Id="MsiBase"/);
  assert.doesNotMatch(source, /TRANSFORMS|SelectedTransform|\.mst/);
  assert.match(source, /MsiProperty Name="INSTALLDIR" Value="\[InstallFolder\]"/);
  assert.doesNotMatch(source, /<Payload Id="Transform/);
  assert.doesNotMatch(source, /InstallCondition=/);
  assert.match(source, /Name="SelectedLanguage" Type="string" Value="" bal:Overridable="yes"/);
  assert.doesNotMatch(source, /Name="SelectedLanguage"[^>]*Persisted="yes"/);
  assert.match(source, /Name="InstallFolder" Type="string" Value="" Persisted="yes"/);
  assert.match(source, /Name="VerifyPlanOnly" Type="numeric" Value="0"/);
  assert.match(source, /Compressed="yes"/);
});

test("managed Burn UI exposes the three-language selector", () => {
  const source = [bootstrapperSourcePath, installerWindowSourcePath, installerWindowXamlPath]
    .map((path) => fs.existsSync(path) ? fs.readFileSync(path, "utf8") : "")
    .join("\n");

  assert.match(source, /简体中文/);
  assert.match(source, /繁體中文/);
  assert.match(source, /English/);
  assert.match(source, /StringVariables\["SelectedLanguage"\]/);
  assert.match(source, /Engine\.Detect\(\)/);
  assert.match(source, /DetectRelatedBundle \+=/);
  assert.match(source, /PlanRelatedBundle \+=/);
  assert.match(source, /RelatedOperation\.None/);
  assert.match(source, /RelationType\.Upgrade/);
  assert.match(source, /e\.State = RequestState\.Absent/);
  assert.match(source, /Engine\.Plan\(/);
  assert.match(source, /Engine\.Apply\(/);
  assert.match(source, /applyWindowHandle == IntPtr\.Zero/);
  assert.match(source, /LaunchAction\.Layout/);
  assert.match(source, /LaunchAction\.Cache/);
  assert.match(source, /commandAction != LaunchAction\.Repair/);
  assert.match(source, /ExitCode = 1602/);
  assert.match(source, /CloseOnUiThread/);
  assert.match(source, /MsiGetProductInfo/);
  assert.match(source, /RegistryView\.Registry64/);
  assert.match(source, /RegistryView\.Registry32/);
  assert.match(source, /SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall/);
  assert.match(source, /"InstallLocation"/);
  assert.match(source, /"DisplayName"/);
  assert.match(source, /"Language"/);
  assert.match(source, /Command\.GetCommandLineArgs\(\)/);
  assert.match(source, /CommandLineValue/);
  assert.match(source, /GetUserDefaultUILanguage/);
  assert.match(source, /GetSystemDefaultUILanguage/);
  assert.doesNotMatch(source, /InstalledProductLanguage/);
});

test("managed Burn falls back unsupported Windows UI languages to English before WPF starts", () => {
  const source = fs.readFileSync(bootstrapperSourcePath, "utf8");
  const script = fs.readFileSync(verifyScriptPath, "utf8");
  const buildScript = fs.readFileSync(buildScriptPath, "utf8");

  assert.match(source, /ResolveLanguageCode\(configuredLanguage\)/);
  assert.match(source, /NormalizeLanguageCode\(name\)/);
  assert.match(source, /return "en-US";/);
  assert.match(source, /ApplySupportedUiCulture\(selectedLanguage\)/);
  assert.match(source, /CultureInfo\.DefaultThreadCurrentUICulture\s*=\s*culture/);
  assert.match(source, /Thread\.CurrentThread\.CurrentUICulture\s*=\s*culture/);
  assert.ok(
    source.indexOf("ApplySupportedUiCulture(selectedLanguage)") < source.indexOf("new Application")
  );
  assert.match(script, /-SelectedLanguage=ja-JP/);
  assert.match(script, /Variable: SelectedLanguage = en-US/);
  assert.match(buildScript, /Release\\net48\\CodeStudioBootstrapper\.dll/);
  assert.doesNotMatch(buildScript, /Get-ChildItem \$baOutputDir -Recurse -Filter "CodeStudioBootstrapper\.dll"/);
});

test("managed Burn reports initialization failures and tolerates legacy install paths", () => {
  const source = fs.readFileSync(bootstrapperSourcePath, "utf8");

  assert.match(source, /protected override void Run\(\)[\s\S]*try[\s\S]*RunInstaller\(\)[\s\S]*catch \(Exception error\)/);
  assert.match(source, /Installer initialization failed:/);
  assert.match(source, /MessageBox\.Show\([\s\S]*Installer could not start/);
  assert.match(source, /ResolveInitialInstallFolder\(/);
  assert.match(source, /TryNormalizeInstallFolder\(/);
  assert.match(source, /ignoring invalid legacy installation location/i);
});

test("interactive Burn brings its clean-room child window to the foreground", () => {
  const source = fs.readFileSync(installerWindowSourcePath, "utf8");

  assert.match(source, /private readonly bool showFullUi/);
  assert.doesNotMatch(source, /ContentRendered \+=/);
  assert.match(source, /Loaded \+= \(_, __\) =>[\s\S]*BringToForeground\(\)/);
  assert.match(source, /BringToForeground\(\)[\s\S]*if \(!showFullUi/);
  assert.match(source, /WindowState = WindowState\.Normal/);
  assert.match(source, /Activate\(\)/);
  assert.match(source, /Topmost = true[\s\S]*Topmost = false/);
  assert.match(source, /SetForegroundWindow\(windowHandle\)/);
});

test("managed Burn UI uses a localized multi-step install wizard", () => {
  const source = [bootstrapperSourcePath, installerWindowSourcePath, installerWindowXamlPath]
    .map((path) => fs.existsSync(path) ? fs.readFileSync(path, "utf8") : "")
    .join("\n");

  assert.match(source, /enum InstallerPage[\s\S]*Welcome[\s\S]*Options[\s\S]*Confirm[\s\S]*Progress[\s\S]*Complete/);
  assert.match(source, /ShowWelcome\(/);
  assert.match(source, /ShowOptions\(/);
  assert.match(source, /ShowConfirmation\(/);
  assert.match(source, /FolderBrowserDialog/);
  assert.match(source, /Engine\.StringVariables\["InstallFolder"\]/);
  assert.match(source, /BeginAction\(string languageCode, string installFolder, LaunchAction action\)/);
  assert.match(source, /"Installation location"/);
  assert.match(source, /"安装位置"/);
  assert.match(source, /"安裝位置"/);
  assert.doesNotMatch(source, /primaryButton\.Click \+= \(_, __\) => bootstrapper\.BeginAction/);
});

test("managed Burn project excludes stale local build intermediates", () => {
  const source = fs.readFileSync(bootstrapperProjectPath, "utf8");

  assert.match(source, /Compile Remove="obj\\\*\*\\\*\.cs"/);
});

test("Burn bootstraps the compact .NET 4.8 web prerequisite", () => {
  const project = fs.readFileSync(bootstrapperProjectPath, "utf8");
  const bundle = fs.readFileSync(bundleSourcePath, "utf8");
  const build = fs.readFileSync(buildScriptPath, "utf8");
  const verify = fs.readFileSync(verifyScriptPath, "utf8");

  assert.match(project, /<TargetFramework>net48<\/TargetFramework>/);
  assert.match(bundle, /<PackageGroupRef Id="NetFx48Web"\s*\/>/);
  assert.doesNotMatch(bundle, /WixMbaPrereqPackageId" Value=""/);
  assert.match(build, /WixNetFxExtension\.dll/);
  assert.ok((build.match(/-ext \$netFxExtension/g) ?? []).length >= 2);
  assert.match(verify, /@\("NetFx48Web", "MsiBase"\)/);
  assert.match(verify, /burn:ExePackage/);
  assert.match(verify, /WixMbaPrereqInformation/);
  assert.match(verify, /PackageId -ne "NetFx48Web"/);
});

test("managed Burn UI uses the CodeStudio Lite WPF theme", () => {
  const project = fs.readFileSync(bootstrapperProjectPath, "utf8");
  const bundle = fs.readFileSync(bundleSourcePath, "utf8");
  const bootstrapper = fs.readFileSync(bootstrapperSourcePath, "utf8");
  const xaml = fs.existsSync(installerWindowXamlPath) ? fs.readFileSync(installerWindowXamlPath, "utf8") : "";

  assert.match(project, /<UseWPF>true<\/UseWPF>/);
  assert.match(bundle, /Name="CodeStudioLite\.ico"/);
  assert.match(bundle, /Name="CodeStudioLite\.png"/);
  assert.match(bootstrapper, /InstallerWindow/);
  assert.doesNotMatch(bootstrapper, /InstallerForm/);
  assert.match(xaml, /WindowStyle="None"/);
  assert.match(xaml, /#0A0B0D/);
  assert.match(xaml, /#101216/);
  assert.match(xaml, /#1F8FFF/);
  assert.match(xaml, /#F4D94E/);
  assert.match(xaml, /x:Name="BrandIcon"/);
  assert.match(xaml, /x:Name="PrimaryButton"/);
  assert.match(xaml, /x:Name="ProgressTrack"/);
  assert.match(xaml, /TargetType="ComboBoxItem"/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /SourceInitialized \+=/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /private IntPtr windowHandle/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /internal IntPtr Handle => windowHandle/);
  assert.doesNotMatch(fs.readFileSync(installerWindowSourcePath, "utf8"), /internal IntPtr Handle => new WindowInteropHelper/);
  assert.match(bootstrapper, /codestudio-lite\.exe/);
  assert.match(bootstrapper, /ProcessStartInfo/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /"Open CodeStudio Lite"/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /"打开 CodeStudio Lite"/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /"Retry"/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /"重试"/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /RetryInstallation\(/);
  assert.match(fs.readFileSync(installerWindowSourcePath, "utf8"), /page == InstallerPage\.Complete[\s\S]*canLaunchInstalledApp \|\| canRetryInstallation/);
});

test("application updates relaunch CodeStudio Lite only after Burn succeeds", () => {
  const backend = fs.readFileSync("src-tauri/src/core/app_updater.rs", "utf8");
  const bootstrapper = fs.readFileSync(bootstrapperSourcePath, "utf8");
  const window = fs.readFileSync(installerWindowSourcePath, "utf8");

  assert.match(backend, /-LaunchAfterInstall=1/);
  assert.match(bootstrapper, /CommandLineValue\("LaunchAfterInstall"\) == "1"/);
  assert.match(bootstrapper, /OnApplyComplete[\s\S]*e\.Status >= 0[\s\S]*launchAfterInstall/);
  assert.match(bootstrapper, /OnApplyComplete[\s\S]*LaunchInstalledApp\(\)/);
  assert.match(bootstrapper, /Process\.Start\(new ProcessStartInfo/);
  assert.match(bootstrapper, /Path\.Combine\(installFolder, "codestudio-lite\.exe"\)/);
  assert.doesNotMatch(window, /Process\.Start/);
});

test("Windows build script creates the Burn bundle from one en-US MSI", () => {
  const script = fs.readFileSync(buildScriptPath, "utf8");

  assert.equal(packageJson.scripts["tauri:build:windows"], "powershell -NoProfile -ExecutionPolicy Bypass -File installer/windows/burn/build-burn.ps1");
  assert.match(script, /npm\.cmd run tauri:build/);
  assert.match(script, /_en-US\.msi/);
  assert.doesNotMatch(script, /_zh-(?:CN|TW)\.msi|torch\.exe|-t language|_Storages|SetStream|Set-MsiProperty|normalized-zh/);
  assert.match(script, /WixBalExtension/);
  assert.match(script, /verify-burn\.ps1/);
  assert.match(script, /CodeStudio-Lite-\$\{version\}-Windows-x64-en-US\.msi/);
  assert.match(script, /CodeStudio-Lite-\$\{version\}-Windows-x64-setup\.exe/);
  assert.match(script, /tauri signer sign.*\$bundleOutput/s);
  assert.match(script, /\$bundleOutput\.sig/);
  assert.match(script, /Remove-StaleLocalizedMsiArtifacts/);
  assert.match(script, /Normalize-WindowsPublishedDirectory/);
  assert.match(script, /\[ _\]\+/);
});

test("Burn verification inspects the built manifest instead of trusting source authoring", () => {
  const script = fs.readFileSync(verifyScriptPath, "utf8");

  assert.match(script, /dark\.exe/);
  assert.match(script, /6B78C4D8-8C90-4C11-A1D8-893160DA17A7/);
  assert.match(script, /MsiBase/);
  assert.doesNotMatch(script, /\.mst|_Storages|TRANSFORMS|SelectedTransform/);
  assert.match(script, /SetARPINSTALLLOCATION/);
  assert.match(script, /ARPINSTALLLOCATION/);
  assert.match(script, /\[INSTALLDIR\]/);
  assert.match(script, /CodeStudioBootstrapper\.dll/);
  assert.match(script, /CodeStudioLite\.ico/);
  assert.match(script, /CodeStudioLite\.png/);
  assert.match(script, /MsiProperty\[@Id='INSTALLDIR'\]/);
  assert.match(script, /-VerifyPlanOnly=1/);
  assert.match(script, /MainWindowHandle/);
  assert.match(script, /MainWindowTitle/);
  assert.match(script, /\.Responding/);
  assert.match(script, /Plan complete, result: 0x0/);
  assert.match(script, /Variable: InstallFolder/);
  assert.match(script, /Variable: SelectedLanguage/);
  assert.doesNotMatch(script, /-SelectedLanguage=zh-CN/);
  assert.match(script, /ba requested: Absent/);
});
