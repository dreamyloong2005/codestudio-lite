using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using Microsoft.Tools.WindowsInstallerXml.Bootstrapper;
using WinForms = System.Windows.Forms;

namespace CodeStudioLite.Installer
{
    internal partial class InstallerWindow : Window
    {
        private static readonly Brush ActiveStepBrush = BrushFrom("#1F8FFF");
        private static readonly Brush InactiveStepBrush = BrushFrom("#424650");
        private static readonly Brush ActiveTextBrush = BrushFrom("#E9EAEC");
        private static readonly Brush InactiveTextBrush = BrushFrom("#A4A8B3");

        private readonly CodeStudioBootstrapperApplication bootstrapper;
        private readonly LaunchAction commandAction;
        private readonly bool showFullUi;
        private InstallerPage page;
        private bool operationComplete;
        private bool detectionComplete;
        private bool canLaunchInstalledApp;
        private bool canRetryInstallation;
        private int progressPercentage;
        private IntPtr windowHandle;

        internal InstallerWindow(
            CodeStudioBootstrapperApplication bootstrapper,
            string initialLanguage,
            string initialFolder,
            LaunchAction commandAction,
            bool showFullUi)
        {
            InitializeComponent();
            this.bootstrapper = bootstrapper;
            this.commandAction = commandAction;
            this.showFullUi = showFullUi;

            LanguageSelector.Items.Add(new InstallerLanguage("zh-CN", "简体中文"));
            LanguageSelector.Items.Add(new InstallerLanguage("zh-TW", "繁體中文"));
            LanguageSelector.Items.Add(new InstallerLanguage("en-US", "English"));
            FolderTextBox.Text = initialFolder;
            SelectLanguage(initialLanguage);
            LoadBrandIcon();
            ShowDetecting();
            SourceInitialized += (_, __) => windowHandle = new WindowInteropHelper(this).Handle;
            Loaded += (_, __) =>
            {
                BringToForeground();
            };
            Closing += OnWindowClosing;

            if (!showFullUi)
            {
                Opacity = 0;
                ShowInTaskbar = false;
                ShowActivated = false;
            }
        }

        internal int ExitCode { get; set; }

        internal IntPtr Handle => windowHandle;

        private void BringToForeground()
        {
            if (!showFullUi || windowHandle == IntPtr.Zero)
            {
                return;
            }

            if (WindowState == WindowState.Minimized)
            {
                WindowState = WindowState.Normal;
            }

            Activate();
            Topmost = true;
            Topmost = false;
            Focus();
            SetForegroundWindow(windowHandle);
        }

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetForegroundWindow(IntPtr windowHandle);

        internal void SelectLanguage(string languageCode)
        {
            OnUiThread(() =>
            {
                for (int i = 0; i < LanguageSelector.Items.Count; i++)
                {
                    if (((InstallerLanguage)LanguageSelector.Items[i]).Code == languageCode)
                    {
                        LanguageSelector.SelectedIndex = i;
                        break;
                    }
                }

                if (detectionComplete)
                {
                    ApplyLocalization();
                }
            });
        }

        internal void SetInstallFolder(string folder) => OnUiThread(() => FolderTextBox.Text = folder);

        internal void ShowWelcome()
        {
            OnUiThread(() =>
            {
                detectionComplete = true;
                page = InstallerPage.Welcome;
                ApplyLocalization();
            });
        }

        internal void ShowOptions()
        {
            page = InstallerPage.Options;
            ApplyLocalization();
        }

        internal void ShowConfirmation()
        {
            if (!TryNormalizeFolder())
            {
                return;
            }

            page = InstallerPage.Confirm;
            ApplyLocalization();
        }

        internal void ShowPlanning(LaunchAction action)
        {
            OnUiThread(() =>
            {
                page = InstallerPage.Progress;
                progressPercentage = 0;
                ApplyLocalization();
                StatusLabel.Text = T("Preparing to make changes...", "正在准备更改...", "正在準備變更...");
                CancelButton.IsEnabled = false;
            });
        }

        internal void ShowApplying()
        {
            OnUiThread(() =>
            {
                page = InstallerPage.Progress;
                ApplyLocalization();
                StatusLabel.Text = T("Applying changes...", "正在应用更改...", "正在套用變更...");
            });
        }

        internal void SetProgress(int percentage)
        {
            OnUiThread(() =>
            {
                progressPercentage = Math.Max(0, Math.Min(100, percentage));
                UpdateProgressWidth();
            });
        }

        internal void ShowComplete(int status, ApplyRestart restart)
        {
            OnUiThread(() =>
            {
                operationComplete = true;
                canLaunchInstalledApp = status >= 0 && (commandAction == LaunchAction.Install || commandAction == LaunchAction.Repair);
                canRetryInstallation = status < 0;
                page = InstallerPage.Complete;
                progressPercentage = status >= 0 ? 100 : progressPercentage;
                ApplyLocalization();

                bool succeeded = status >= 0;
                CompleteGlyph.Text = succeeded ? "✓" : "!";
                CompleteGlyph.Foreground = BrushFrom(succeeded ? "#65D69B" : "#FF9A93");
                CompleteMessage.Text = succeeded
                    ? (restart == ApplyRestart.RestartRequired
                        ? T("Restart Windows to finish setup.", "请重启 Windows 以完成安装。", "請重新啟動 Windows 以完成安裝。")
                        : T("Setup completed successfully.", "安装已成功完成。", "安裝已成功完成。"))
                    : T("Setup failed", "安装失败", "安裝失敗") + " (0x" + status.ToString("X8") + ").";
            });
        }

        internal void ShowFailure(int status, string message)
        {
            OnUiThread(() =>
            {
                operationComplete = true;
                canLaunchInstalledApp = false;
                canRetryInstallation = false;
                ExitCode = status;
                page = InstallerPage.Complete;
                ApplyLocalization();
                CompleteGlyph.Text = "!";
                CompleteGlyph.Foreground = BrushFrom("#FF9A93");
                CompleteMessage.Text = message + " (0x" + status.ToString("X8") + ")";
            });
        }

        internal void ShowInvalidInstallFolder()
        {
            OnUiThread(() =>
            {
                page = InstallerPage.Options;
                ApplyLocalization();
                MessageBox.Show(
                    this,
                    T("Enter a valid absolute installation path.", "请输入有效的绝对安装路径。", "請輸入有效的絕對安裝路徑。"),
                    T("Invalid installation location", "安装位置无效", "安裝位置無效"),
                    MessageBoxButton.OK,
                    MessageBoxImage.Warning);
                FolderTextBox.Focus();
            });
        }

        internal void CloseOnUiThread() => OnUiThread(Close);

        internal void CloseAfterPlanVerification()
        {
            OnUiThread(() =>
            {
                operationComplete = true;
                ExitCode = 0;
                Close();
            });
        }

        private void ShowDetecting()
        {
            page = InstallerPage.Progress;
            progressPercentage = 0;
            HeadingLabel.Text = "CodeStudio Lite";
            DescriptionLabel.Text = "Checking the current installation...";
            StatusLabel.Text = "Please wait.";
            PrimaryButton.IsEnabled = false;
            CancelButton.Content = "Cancel";
            SetPageVisibility();
            UpdateStepIndicator();
        }

        private void PrimaryButtonClick(object sender, RoutedEventArgs e) => Advance();

        private void BackButtonClick(object sender, RoutedEventArgs e) => GoBack();

        private void CancelButtonClick(object sender, RoutedEventArgs e) => CloseOrCancel();

        private void CloseButtonClick(object sender, RoutedEventArgs e) => CloseOrCancel();

        private void BrowseButtonClick(object sender, RoutedEventArgs e) => BrowseForInstallFolder();

        private void LanguageSelectionChanged(object sender, System.Windows.Controls.SelectionChangedEventArgs e)
        {
            if (detectionComplete)
            {
                ApplyLocalization();
            }
        }

        private void DragWindow(object sender, MouseButtonEventArgs e)
        {
            if (e.LeftButton == MouseButtonState.Pressed)
            {
                DragMove();
            }
        }

        private void ProgressTrackSizeChanged(object sender, SizeChangedEventArgs e) => UpdateProgressWidth();

        private void Advance()
        {
            switch (page)
            {
                case InstallerPage.Welcome:
                    if (commandAction == LaunchAction.Uninstall || commandAction == LaunchAction.Repair)
                    {
                        ShowConfirmation();
                    }
                    else
                    {
                        ShowOptions();
                    }
                    break;
                case InstallerPage.Options:
                    ShowConfirmation();
                    break;
                case InstallerPage.Confirm:
                    bootstrapper.BeginAction(SelectedLanguageCode(), FolderTextBox.Text, commandAction);
                    break;
                case InstallerPage.Complete:
                    if (canLaunchInstalledApp)
                    {
                        LaunchInstalledApp();
                    }
                    else if (canRetryInstallation)
                    {
                        RetryInstallation();
                    }
                    else
                    {
                        Close();
                    }
                    break;
            }
        }

        private void GoBack()
        {
            if (page == InstallerPage.Confirm && commandAction != LaunchAction.Uninstall && commandAction != LaunchAction.Repair)
            {
                ShowOptions();
            }
            else
            {
                ShowWelcome();
            }
        }

        private void BrowseForInstallFolder()
        {
            using (var dialog = new WinForms.FolderBrowserDialog
            {
                Description = T("Installation location", "安装位置", "安裝位置"),
                SelectedPath = FolderTextBox.Text,
                ShowNewFolderButton = true,
            })
            {
                if (dialog.ShowDialog(new NativeWindowOwner(Handle)) == WinForms.DialogResult.OK)
                {
                    FolderTextBox.Text = dialog.SelectedPath;
                }
            }
        }

        private bool TryNormalizeFolder()
        {
            try
            {
                if (string.IsNullOrWhiteSpace(FolderTextBox.Text) || !Path.IsPathRooted(FolderTextBox.Text))
                {
                    throw new ArgumentException();
                }

                FolderTextBox.Text = Path.GetFullPath(FolderTextBox.Text.Trim().Trim('"'))
                    .TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
                return true;
            }
            catch (ArgumentException)
            {
                ShowInvalidInstallFolder();
                return false;
            }
            catch (NotSupportedException)
            {
                ShowInvalidInstallFolder();
                return false;
            }
        }

        private void ApplyLocalization()
        {
            Title = T("CodeStudio Lite Setup", "CodeStudio Lite 安装程序", "CodeStudio Lite 安裝程式");
            WindowTitle.Text = Title;
            BrandSubtitle.Text = T("Desktop AI workspace", "桌面 AI 工作空间", "桌面 AI 工作空間");
            LanguageLabel.Text = T("Installer language", "安装程序语言", "安裝程式語言");
            FolderLabel.Text = T("Installation location", "安装位置", "安裝位置");
            BrowseButton.Content = T("Browse...", "浏览...", "瀏覽...");
            FolderHint.Text = T("Updates will keep using this location.", "后续更新将继续使用此位置。", "後續更新將繼續使用此位置。");
            BackButton.Content = T("Back", "返回", "返回");
            CancelButton.Content = operationComplete ? T("Close", "关闭", "關閉") : T("Cancel", "取消", "取消");
            StepWelcomeText.Text = T("Welcome", "欢迎", "歡迎");
            StepOptionsText.Text = T("Options", "选项", "選項");
            StepConfirmText.Text = T("Confirm", "确认", "確認");
            StepProgressText.Text = T("Install", "安装", "安裝");

            switch (page)
            {
                case InstallerPage.Welcome:
                    HeadingLabel.Text = T("Welcome", "欢迎", "歡迎");
                    DescriptionLabel.Text = T("Set up CodeStudio Lite in a few steps.", "只需几步即可完成 CodeStudio Lite 安装。", "只需幾步即可完成 CodeStudio Lite 安裝。");
                    WelcomeHint.Text = T("Choose the language used by this installer. The application language can still be changed later.", "请选择安装程序使用的语言。应用语言仍可在安装后更改。", "請選擇安裝程式使用的語言。應用程式語言仍可在安裝後變更。");
                    PrimaryButton.Content = T("Next", "下一步", "下一步");
                    break;
                case InstallerPage.Options:
                    HeadingLabel.Text = T("Installation options", "安装选项", "安裝選項");
                    DescriptionLabel.Text = T("Choose where CodeStudio Lite will be installed.", "选择 CodeStudio Lite 的安装位置。", "選擇 CodeStudio Lite 的安裝位置。");
                    PrimaryButton.Content = T("Next", "下一步", "下一步");
                    break;
                case InstallerPage.Confirm:
                    HeadingLabel.Text = T("Ready to continue", "准备继续", "準備繼續");
                    DescriptionLabel.Text = T("Review the settings before setup makes changes.", "请在安装程序进行更改前检查设置。", "請在安裝程式進行變更前檢查設定。");
                    SummaryTitle.Text = T("Installation summary", "安装摘要", "安裝摘要");
                    SummaryLanguageLabel.Text = T("Language", "语言", "語言");
                    SummaryFolderLabel.Text = T("Location", "位置", "位置");
                    SummaryLanguageValue.Text = ((InstallerLanguage)LanguageSelector.SelectedItem).Label;
                    SummaryFolderValue.Text = FolderTextBox.Text;
                    PrimaryButton.Content = ActionLabel();
                    break;
                case InstallerPage.Progress:
                    HeadingLabel.Text = T("Setup in progress", "正在安装", "正在安裝");
                    DescriptionLabel.Text = T("CodeStudio Lite is applying the requested changes.", "CodeStudio Lite 正在应用所需更改。", "CodeStudio Lite 正在套用所需變更。");
                    PrimaryButton.Content = ActionLabel();
                    break;
                case InstallerPage.Complete:
                    HeadingLabel.Text = T("Setup complete", "安装完成", "安裝完成");
                    DescriptionLabel.Text = T("CodeStudio Lite setup has finished.", "CodeStudio Lite 安装程序已完成。", "CodeStudio Lite 安裝程式已完成。");
                    PrimaryButton.Content = canLaunchInstalledApp
                        ? T("Open CodeStudio Lite", "打开 CodeStudio Lite", "開啟 CodeStudio Lite")
                        : (canRetryInstallation
                            ? T("Retry", "重试", "重試")
                            : T("Close", "关闭", "關閉"));
                    break;
            }

            SetPageVisibility();
            UpdateStepIndicator();
        }

        private void SetPageVisibility()
        {
            WelcomePanel.Visibility = page == InstallerPage.Welcome ? Visibility.Visible : Visibility.Collapsed;
            OptionsPanel.Visibility = page == InstallerPage.Options ? Visibility.Visible : Visibility.Collapsed;
            ConfirmPanel.Visibility = page == InstallerPage.Confirm ? Visibility.Visible : Visibility.Collapsed;
            ProgressPanel.Visibility = page == InstallerPage.Progress ? Visibility.Visible : Visibility.Collapsed;
            CompletePanel.Visibility = page == InstallerPage.Complete ? Visibility.Visible : Visibility.Collapsed;

            LanguageSelector.IsEnabled = detectionComplete && commandAction != LaunchAction.Uninstall && commandAction != LaunchAction.Repair;
            BackButton.Visibility = page == InstallerPage.Options || page == InstallerPage.Confirm ? Visibility.Visible : Visibility.Collapsed;
            BackButton.IsEnabled = page != InstallerPage.Progress && !operationComplete;
            PrimaryButton.Visibility = page == InstallerPage.Complete
                ? (canLaunchInstalledApp || canRetryInstallation ? Visibility.Visible : Visibility.Collapsed)
                : (page != InstallerPage.Progress ? Visibility.Visible : Visibility.Collapsed);
            PrimaryButton.IsEnabled = detectionComplete && page != InstallerPage.Progress;
            CancelButton.IsEnabled = page != InstallerPage.Progress || operationComplete;
        }

        private void UpdateStepIndicator()
        {
            int activeStep;
            switch (page)
            {
                case InstallerPage.Options: activeStep = 1; break;
                case InstallerPage.Confirm: activeStep = 2; break;
                case InstallerPage.Progress:
                case InstallerPage.Complete: activeStep = 3; break;
                default: activeStep = 0; break;
            }

            SetStepState(StepWelcomeDot, StepWelcomeText, activeStep >= 0, activeStep == 0);
            SetStepState(StepOptionsDot, StepOptionsText, activeStep >= 1, activeStep == 1);
            SetStepState(StepConfirmDot, StepConfirmText, activeStep >= 2, activeStep == 2);
            SetStepState(StepProgressDot, StepProgressText, activeStep >= 3, activeStep == 3);
        }

        private static void SetStepState(System.Windows.Controls.Border dot, System.Windows.Controls.TextBlock label, bool reached, bool active)
        {
            dot.Background = reached ? ActiveStepBrush : InactiveStepBrush;
            label.Foreground = active || reached ? ActiveTextBrush : InactiveTextBrush;
            label.FontWeight = active ? FontWeights.SemiBold : FontWeights.Normal;
        }

        private void UpdateProgressWidth()
        {
            double availableWidth = Math.Max(0, ProgressTrack.ActualWidth);
            ProgressFill.Width = availableWidth * progressPercentage / 100.0;
            ProgressPercent.Text = progressPercentage + "%";
        }

        private string ActionLabel()
        {
            if (commandAction == LaunchAction.Uninstall) return T("Uninstall", "卸载", "解除安裝");
            if (commandAction == LaunchAction.Repair) return T("Repair", "修复", "修復");
            return T("Install", "安装", "安裝");
        }

        private void LaunchInstalledApp()
        {
            try
            {
                bootstrapper.LaunchInstalledApp();
                Close();
            }
            catch (Exception error)
            {
                MessageBox.Show(
                    this,
                    T("CodeStudio Lite could not be opened: ", "无法打开 CodeStudio Lite：", "無法開啟 CodeStudio Lite：") + error.Message,
                    T("Launch failed", "启动失败", "啟動失敗"),
                    MessageBoxButton.OK,
                    MessageBoxImage.Error);
            }
        }

        private void RetryInstallation()
        {
            operationComplete = false;
            canLaunchInstalledApp = false;
            canRetryInstallation = false;
            ExitCode = 0;
            bootstrapper.BeginAction(SelectedLanguageCode(), FolderTextBox.Text, commandAction);
        }

        private string SelectedLanguageCode()
        {
            var language = LanguageSelector.SelectedItem as InstallerLanguage;
            return language?.Code ?? "en-US";
        }

        private string T(string english, string simplifiedChinese, string traditionalChinese) =>
            CodeStudioBootstrapperApplication.Localized(SelectedLanguageCode(), english, simplifiedChinese, traditionalChinese);

        private void CloseOrCancel()
        {
            if (operationComplete)
            {
                Close();
            }
            else
            {
                bootstrapper.Cancel();
            }
        }

        private void OnWindowClosing(object sender, CancelEventArgs e)
        {
            if (!operationComplete && !CancelButton.IsEnabled)
            {
                e.Cancel = true;
                return;
            }

            if (!operationComplete)
            {
                ExitCode = 1602;
            }
        }

        private void LoadBrandIcon()
        {
            string iconPath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "CodeStudioLite.ico");
            if (!File.Exists(iconPath))
            {
                return;
            }

            var image = BitmapFrame.Create(
                new Uri(iconPath, UriKind.Absolute),
                BitmapCreateOptions.PreservePixelFormat,
                BitmapCacheOption.OnLoad);
            Icon = image;
            TitleIcon.Source = image;

            string brandImagePath = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "CodeStudioLite.png");
            BrandIcon.Source = File.Exists(brandImagePath)
                ? BitmapFrame.Create(new Uri(brandImagePath, UriKind.Absolute), BitmapCreateOptions.PreservePixelFormat, BitmapCacheOption.OnLoad)
                : image;
        }

        private void OnUiThread(Action action)
        {
            if (Dispatcher.HasShutdownStarted || Dispatcher.HasShutdownFinished)
            {
                return;
            }

            if (Dispatcher.CheckAccess())
            {
                action();
            }
            else
            {
                Dispatcher.BeginInvoke(action);
            }
        }

        private static Brush BrushFrom(string color)
        {
            var brush = new SolidColorBrush((Color)ColorConverter.ConvertFromString(color));
            brush.Freeze();
            return brush;
        }

        private sealed class NativeWindowOwner : WinForms.IWin32Window
        {
            internal NativeWindowOwner(IntPtr handle)
            {
                Handle = handle;
            }

            public IntPtr Handle { get; }
        }
    }
}
