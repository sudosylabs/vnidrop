using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using Microsoft.Windows.Storage.Pickers;
using VniDrop.Core;
using VniDrop.Platform;

namespace VniDrop.Views.Settings;

public sealed partial class PreferencesPage : Page
{
    private const PreferenceWriteScope OwnedPreferenceScope =
        PreferenceWriteScope.Username | PreferenceWriteScope.ReceiveDirectory;
    private CancellationTokenSource writeLifetime = new();
    private readonly DispatcherQueueTimer usernameDebounce;
    private bool initializing = true;
    private bool saving;
    private bool saveRequested;
    private Task saveOperation = Task.CompletedTask;
    private long usernameRevision;
    private long eligibleUsernameRevision;
    private long savedUsernameRevision;
    private long failedUsernameRevision;
    private string eligibleUsername = "";
    private long folderRevision;
    private long savedFolderRevision;
    private long failedFolderRevision;
    private string? reportedError;

    public PreferencesPage()
    {
        InitializeComponent();
        usernameDebounce = DispatcherQueue.CreateTimer();
        usernameDebounce.Interval = TimeSpan.FromMilliseconds(350);
        usernameDebounce.IsRepeating = false;
        usernameDebounce.Tick += UsernameDebounceElapsed;
    }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        if (writeLifetime.IsCancellationRequested)
        {
            writeLifetime.Dispose();
            writeLifetime = new();
        }
        var lifetime = writeLifetime.Token;
        initializing = true;
        usernameDebounce.Stop();
        IsEnabled = false;
        Saving.Visibility = Visibility.Visible;
        Saving.IsActive = true;
        try
        {
            await App.Window.Model.FlushPreferenceWritesAsync();
        }
        catch (Exception ex)
        {
            if (!lifetime.IsCancellationRequested) App.Window.Model.Report(ex);
        }
        if (lifetime.IsCancellationRequested) return;

        var preferences = App.Window.Model.Preferences;
        Username.Text = preferences.Username;
        ReceiveFolder.Text = preferences.ReceiveDirectory;
        usernameRevision = eligibleUsernameRevision = savedUsernameRevision = 0;
        failedUsernameRevision = 0;
        folderRevision = savedFolderRevision = failedFolderRevision = 0;
        eligibleUsername = preferences.Username;
        saveRequested = false;
        reportedError = null;
        Saving.IsActive = false;
        Saving.Visibility = Visibility.Collapsed;
        IsEnabled = true;
        initializing = false;
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        usernameDebounce.Stop();
        MakeUsernameEligible();
        QueueFinalSaveBeforeAbandon();
        writeLifetime.Cancel();
        App.Window.Model.DiscardPreferenceWriteFailure(OwnedPreferenceScope);
        base.OnNavigatedFrom(e);
    }

    public async Task<bool> FlushAsync()
    {
        IsEnabled = false;
        try
        {
            usernameDebounce.Stop();
            while (true)
            {
                MakeUsernameEligible();
                RequestSave();
                if (saving || saveRequested || !saveOperation.IsCompleted)
                {
                    var operation = saveOperation;
                    await operation;
                    if (saveRequested && !saving) saveOperation = SavePendingChangesAsync(writeLifetime.Token);
                    continue;
                }
                return usernameRevision <= savedUsernameRevision && folderRevision <= savedFolderRevision;
            }
        }
        finally { IsEnabled = true; }
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();

    private void UsernameChanged(object sender, TextChangedEventArgs e)
    {
        if (initializing) return;
        usernameRevision++;
        usernameDebounce.Stop();
        usernameDebounce.Start();
    }

    private void UsernameLostFocus(object sender, RoutedEventArgs e)
    {
        usernameDebounce.Stop();
        MakeUsernameEligible();
        RequestSave();
    }

    private void UsernameDebounceElapsed(DispatcherQueueTimer sender, object args)
    {
        sender.Stop();
        if (MakeUsernameEligible()) RequestSave();
    }

    private bool MakeUsernameEligible()
    {
        if (initializing) return false;
        var current = Username.Text;
        if (usernameRevision <= eligibleUsernameRevision
            && string.Equals(current, eligibleUsername, StringComparison.Ordinal)) return false;
        if (usernameRevision <= eligibleUsernameRevision)
            usernameRevision = eligibleUsernameRevision + 1;
        eligibleUsernameRevision = usernameRevision;
        eligibleUsername = current;
        return true;
    }

    private async void ChooseFolder(object sender, RoutedEventArgs e)
    {
        try
        {
            var folder = await new FolderPicker(App.Window.AppWindow.Id).PickSingleFolderAsync();
            if (folder is not null) UpdateReceiveFolder(folder.Path);
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
    }

    private void ResetFolder(object sender, RoutedEventArgs e) => UpdateReceiveFolder(WindowsFiles.DownloadsDirectory());

    private void UpdateReceiveFolder(string path)
    {
        if (string.Equals(ReceiveFolder.Text, path, StringComparison.OrdinalIgnoreCase))
        {
            RequestSave();
            return;
        }
        ReceiveFolder.Text = path;
        folderRevision++;
        RequestSave();
    }

    private void RequestSave()
    {
        var usernamePending = eligibleUsernameRevision > savedUsernameRevision
            && eligibleUsernameRevision != failedUsernameRevision;
        var folderPending = folderRevision > savedFolderRevision
            && folderRevision != failedFolderRevision;
        if (!usernamePending && !folderPending) return;
        saveRequested = true;
        if (!saving) saveOperation = SavePendingChangesAsync(writeLifetime.Token);
    }

    private void QueueFinalSaveBeforeAbandon()
    {
        var usernameAttempt = eligibleUsernameRevision > savedUsernameRevision
            && eligibleUsernameRevision != failedUsernameRevision ? eligibleUsernameRevision : 0;
        var folderAttempt = folderRevision > savedFolderRevision
            && folderRevision != failedFolderRevision ? folderRevision : 0;
        saveRequested = false;
        if (usernameAttempt == 0 && folderAttempt == 0) return;

        var candidate = App.Window.Model.Preferences;
        var scope = PreferenceWriteScope.None;
        if (usernameAttempt != 0)
        {
            var username = eligibleUsername.Trim();
            if (username.Length == 0)
            {
                ReportSaveError(new InvalidDataException("windows_name_required"));
                failedUsernameRevision = usernameAttempt;
            }
            else
            {
                candidate = candidate with { Username = username };
                scope |= PreferenceWriteScope.Username;
            }
        }
        if (folderAttempt != 0)
        {
            candidate = candidate with { ReceiveDirectory = ReceiveFolder.Text };
            scope |= PreferenceWriteScope.ReceiveDirectory;
        }
        if (scope == PreferenceWriteScope.None) return;

        _ = App.Window.Model.SavePreferencesAsync(candidate, scope, writeLifetime.Token);
    }

    private async Task SavePendingChangesAsync(CancellationToken abandonmentToken)
    {
        saving = true;
        Saving.Visibility = Visibility.Visible;
        Saving.IsActive = true;
        try
        {
            while (saveRequested && !abandonmentToken.IsCancellationRequested)
            {
                saveRequested = false;
                var usernameAttempt = eligibleUsernameRevision > savedUsernameRevision
                    && eligibleUsernameRevision != failedUsernameRevision ? eligibleUsernameRevision : 0;
                var folderAttempt = folderRevision > savedFolderRevision
                    && folderRevision != failedFolderRevision ? folderRevision : 0;
                if (usernameAttempt == 0 && folderAttempt == 0) continue;

                var current = App.Window.Model.Preferences;
                var candidate = current;
                var includesUsername = usernameAttempt != 0;
                var includesFolder = folderAttempt != 0;

                if (includesUsername)
                {
                    var username = eligibleUsername.Trim();
                    if (username.Length == 0)
                    {
                        ReportSaveError(new InvalidDataException("windows_name_required"));
                        failedUsernameRevision = usernameAttempt;
                        includesUsername = false;
                    }
                    else candidate = candidate with { Username = username };
                }
                if (includesFolder) candidate = candidate with { ReceiveDirectory = ReceiveFolder.Text };
                if (!includesUsername && !includesFolder) continue;

                try
                {
                    var scope = PreferenceWriteScope.None;
                    if (includesUsername) scope |= PreferenceWriteScope.Username;
                    if (includesFolder) scope |= PreferenceWriteScope.ReceiveDirectory;
                    await App.Window.Model.SavePreferencesAsync(candidate, scope, abandonmentToken);
                    if (includesUsername)
                    {
                        savedUsernameRevision = usernameAttempt;
                        failedUsernameRevision = 0;
                    }
                    if (includesFolder)
                    {
                        savedFolderRevision = folderAttempt;
                        failedFolderRevision = 0;
                    }
                    if (eligibleUsernameRevision <= savedUsernameRevision && folderRevision <= savedFolderRevision)
                        ClearReportedError();
                }
                catch (Exception ex)
                {
                    if (includesUsername) failedUsernameRevision = usernameAttempt;
                    if (includesFolder) failedFolderRevision = folderAttempt;
                    ReportSaveError(ex);
                    return;
                }

                if (!abandonmentToken.IsCancellationRequested
                    && (eligibleUsernameRevision > savedUsernameRevision || folderRevision > savedFolderRevision))
                    saveRequested = true;
            }
        }
        finally
        {
            saving = false;
            Saving.IsActive = false;
            Saving.Visibility = Visibility.Collapsed;
            if (saveRequested && !abandonmentToken.IsCancellationRequested)
                saveOperation = SavePendingChangesAsync(abandonmentToken);
        }
    }

    private void ReportSaveError(Exception error)
    {
        App.Window.Model.Report(error);
        reportedError = App.Window.Model.Error;
    }

    private void ClearReportedError()
    {
        if (reportedError is not null && App.Window.Model.Error == reportedError)
            App.Window.Model.ClearError();
        reportedError = null;
    }
}
