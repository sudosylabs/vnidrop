using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using VniDrop.Core;
using VniDrop.Platform;
using VniDrop.Services;

namespace VniDrop.Views.Settings;

public sealed partial class StoragePage : Page
{
    private bool working;

    public StoragePage() => InitializeComponent();

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        _ = LoadStorageAsync();
    }

    private void Back(object sender, RoutedEventArgs e) => App.Window.GoBack();
    private async void RefreshStorage(object sender, RoutedEventArgs e) => await LoadStorageAsync();

    private async Task LoadStorageAsync()
    {
        if (working) return;
        if (!App.Window.Model.Ready)
        {
            StorageStatus.Text = Strings.Get("storage_unavailable");
            return;
        }

        working = true;
        SetActionsEnabled(false);
        StorageLoading.Visibility = Visibility.Visible;
        StorageLoading.IsActive = true;
        StorageStatus.Text = Strings.Get("storage_calculating");
        try
        {
            var core = await App.Window.Model.Session.RunAsync(c => c.StorageUsage());
            var artifacts = await App.Window.Model.Session.RunAsync(c => c.ListReceivedArtifacts());
            var storage = await Task.Run(() => WindowsStorage.Inspect(
                App.Window.Model.Session.ProfileDirectory,
                App.Window.Model.Preferences.ReceiveDirectory,
                core,
                artifacts));
            ReceivedUsage.Value = Strings.Size(storage.Received);
            TransferUsage.Value = Strings.Size(storage.Transfer);
            AppUsage.Value = Strings.Size(storage.AppData);
            TemporaryUsage.Value = Strings.Size(storage.Temporary);
            TotalUsage.Value = Strings.Size(storage.Total);
            StorageStatus.Text = "";
        }
        catch (Exception ex)
        {
            StorageStatus.Text = Strings.Get("storage_unavailable");
            App.Window.Model.Report(ex);
        }
        finally
        {
            StorageLoading.IsActive = false;
            StorageLoading.Visibility = Visibility.Collapsed;
            working = false;
            SetActionsEnabled(true);
        }
    }

    private async void FreeSpace(object sender, RoutedEventArgs e)
    {
        if (!BeginWork(FreeSpaceRow, "storage_cleaning")) return;
        try
        {
            var facts = await App.Window.Model.Session.RunAsync(c => c.RuntimeObligationFacts());
            if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability + facts.targetedProviderAvailability + facts.targetedPreparations > 0)
                throw new InvalidOperationException("windows_network_busy");
            await Task.Run(() => WindowsStorage.ReclaimTemporary(
                App.Window.Model.Session.ProfileDirectory,
                App.Window.Model.Preferences.ReceiveDirectory));
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally
        {
            EndWork();
            await LoadStorageAsync();
        }
    }

    private async void ClearCache(object sender, RoutedEventArgs e)
    {
        if (await App.Window.DialogAsync(
                Strings.Get("storage_clear_transfer_cache"),
                Strings.Get("storage_clear_transfer_cache_description"),
                Strings.Get("storage_clear_transfer_cache"),
                intent: DialogIntent.Destructive) != ContentDialogResult.Primary ||
            !BeginWork(ClearCacheRow, "storage_clearing_transfer_cache")) return;
        try { await App.Window.Model.ClearCacheAsync(); }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally
        {
            EndWork();
            await LoadStorageAsync();
        }
    }

    private async void DeleteAll(object sender, RoutedEventArgs e)
    {
        if (await App.Window.DialogAsync(
                Strings.Get("storage_delete_transfers"),
                Strings.Get("storage_delete_transfers_description"),
                Strings.Get("storage_delete_transfers"),
                intent: DialogIntent.Destructive) != ContentDialogResult.Primary ||
            !BeginWork(DeleteAllRow, "storage_deleting")) return;
        try
        {
            var snapshot = App.Window.Model.Snapshot;
            var ids = TransferPresentation.DeletableHistoryIds(snapshot?.Transfers ?? []);
            var targetedIds = TransferPresentation.DeletableTargetedHistoryIds(snapshot?.TargetedTransfers ?? []);
            foreach (var id in ids) await App.Window.Model.Session.RunAsync(c => c.DeleteTransfer(id));
            foreach (var id in targetedIds) await App.Window.Model.Session.RunAsync(c => c.DeleteTargetedTransfer(id));
            await App.Window.Model.RefreshAsync(true);
            var facts = await App.Window.Model.Session.RunAsync(c => c.RuntimeObligationFacts());
            if (facts.activeInvitationTransfers + facts.activeTargetedTransfers + facts.invitationProviderAvailability
                + facts.targetedProviderAvailability + facts.targetedPreparations == 0)
                await App.Window.Model.ClearCacheAsync();
        }
        catch (Exception ex) { App.Window.Model.Report(ex); }
        finally
        {
            EndWork();
            await LoadStorageAsync();
        }
    }

    private bool BeginWork(VniDrop.Controls.SettingsRow activeRow, string statusKey)
    {
        if (working) return false;
        working = true;
        SetActionsEnabled(false);
        activeRow.Title = Strings.Get(statusKey);
        return true;
    }

    private void EndWork()
    {
        working = false;
        FreeSpaceRow.Title = Strings.Get("storage_free_up_space");
        ClearCacheRow.Title = Strings.Get("storage_clear_transfer_cache");
        DeleteAllRow.Title = Strings.Get("storage_delete_transfers");
        SetActionsEnabled(true);
    }

    private void SetActionsEnabled(bool enabled) =>
        FreeSpaceRow.IsEnabled = ClearCacheRow.IsEnabled = DeleteAllRow.IsEnabled = RefreshButton.IsEnabled = enabled;
}
