package com.vnidrop.app.ui.icons

import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.ui.platform.LocalUiPlatform
import org.jetbrains.compose.resources.DrawableResource
import org.jetbrains.compose.resources.painterResource
import vnidrop.shared.generated.resources.*

internal enum class AppIcon(
	val material: DrawableResource,
	val fluent: DrawableResource,
	val lucide: DrawableResource,
) {
	Add(Res.drawable.icon_material_add, Res.drawable.icon_fluent_add, Res.drawable.icon_lucide_add),
	ArrowBack(Res.drawable.icon_material_arrow_back, Res.drawable.icon_fluent_arrow_back, Res.drawable.icon_lucide_arrow_back),
	Bell(Res.drawable.icon_material_bell, Res.drawable.icon_fluent_bell, Res.drawable.icon_lucide_bell),
	Bug(Res.drawable.icon_material_bug, Res.drawable.icon_fluent_bug, Res.drawable.icon_lucide_bug),
	Check(Res.drawable.icon_material_check, Res.drawable.icon_fluent_check, Res.drawable.icon_lucide_check),
	ChevronRight(
		Res.drawable.icon_material_chevron_right,
		Res.drawable.icon_fluent_chevron_right,
		Res.drawable.icon_lucide_chevron_right,
	),
	Close(Res.drawable.icon_material_close, Res.drawable.icon_fluent_close, Res.drawable.icon_lucide_close),
	Code(Res.drawable.icon_material_code, Res.drawable.icon_fluent_code, Res.drawable.icon_lucide_code),
	CloudOff(Res.drawable.icon_material_cloud_off, Res.drawable.icon_fluent_cloud_off, Res.drawable.icon_lucide_cloud_off),
	Delete(Res.drawable.icon_material_delete, Res.drawable.icon_fluent_delete, Res.drawable.icon_lucide_delete),
	Document(Res.drawable.icon_material_document, Res.drawable.icon_fluent_document, Res.drawable.icon_lucide_document),
	Download(Res.drawable.icon_material_download, Res.drawable.icon_fluent_download, Res.drawable.icon_lucide_download),
	File(Res.drawable.icon_material_file, Res.drawable.icon_fluent_file, Res.drawable.icon_lucide_file),
	Folder(Res.drawable.icon_material_folder, Res.drawable.icon_fluent_folder, Res.drawable.icon_lucide_folder),
	Globe(Res.drawable.icon_material_globe, Res.drawable.icon_fluent_globe, Res.drawable.icon_lucide_globe),
	Hand(Res.drawable.icon_material_hand, Res.drawable.icon_fluent_hand, Res.drawable.icon_lucide_hand),
	Info(Res.drawable.icon_material_info, Res.drawable.icon_fluent_info, Res.drawable.icon_lucide_info),
	Lock(Res.drawable.icon_material_lock, Res.drawable.icon_fluent_lock, Res.drawable.icon_lucide_lock),
	Megaphone(Res.drawable.icon_material_megaphone, Res.drawable.icon_fluent_megaphone, Res.drawable.icon_lucide_megaphone),
	MoreVertical(
		Res.drawable.icon_material_more_vertical,
		Res.drawable.icon_fluent_more_vertical,
		Res.drawable.icon_lucide_more_vertical,
	),
	Moon(Res.drawable.icon_material_moon, Res.drawable.icon_fluent_moon, Res.drawable.icon_lucide_moon),
	Nfc(Res.drawable.icon_material_nfc, Res.drawable.icon_fluent_nfc, Res.drawable.icon_lucide_nfc),
	QrCode(Res.drawable.icon_material_qr_code, Res.drawable.icon_fluent_qr_code, Res.drawable.icon_lucide_qr_code),
	Radio(Res.drawable.icon_material_radio, Res.drawable.icon_fluent_radio, Res.drawable.icon_lucide_radio),
	Scan(Res.drawable.icon_material_scan, Res.drawable.icon_fluent_scan, Res.drawable.icon_lucide_scan),
	Send(Res.drawable.icon_material_send, Res.drawable.icon_fluent_send, Res.drawable.icon_lucide_send),
	Share(Res.drawable.icon_material_share, Res.drawable.icon_fluent_share, Res.drawable.icon_lucide_share),
	Sparkles(Res.drawable.icon_material_sparkles, Res.drawable.icon_fluent_sparkles, Res.drawable.icon_lucide_sparkles),
	Settings(Res.drawable.icon_material_settings, Res.drawable.icon_fluent_settings, Res.drawable.icon_lucide_settings),
	Shield(Res.drawable.icon_material_shield, Res.drawable.icon_fluent_shield, Res.drawable.icon_lucide_shield),
	ShieldCheck(
		Res.drawable.icon_material_shield_check,
		Res.drawable.icon_fluent_shield_check,
		Res.drawable.icon_lucide_shield_check,
	),
	StopCircle(
		Res.drawable.icon_material_stop_circle,
		Res.drawable.icon_fluent_stop_circle,
		Res.drawable.icon_lucide_stop_circle,
	),
	Storage(Res.drawable.icon_material_storage, Res.drawable.icon_fluent_storage, Res.drawable.icon_lucide_storage),
	Sun(Res.drawable.icon_material_sun, Res.drawable.icon_fluent_sun, Res.drawable.icon_lucide_sun),
	Sync(Res.drawable.icon_material_sync, Res.drawable.icon_fluent_sync, Res.drawable.icon_lucide_sync),
	SystemTheme(
		Res.drawable.icon_material_system_theme,
		Res.drawable.icon_fluent_system_theme,
		Res.drawable.icon_lucide_system_theme,
	),
	Temporary(
		Res.drawable.icon_material_temporary,
		Res.drawable.icon_fluent_temporary,
		Res.drawable.icon_lucide_temporary,
	),
	TotalStorage(
		Res.drawable.icon_material_total_storage,
		Res.drawable.icon_fluent_total_storage,
		Res.drawable.icon_lucide_total_storage,
	),
	TransferData(
		Res.drawable.icon_material_transfer_data,
		Res.drawable.icon_fluent_transfer_data,
		Res.drawable.icon_lucide_transfer_data,
	),
	User(Res.drawable.icon_material_user, Res.drawable.icon_fluent_user, Res.drawable.icon_lucide_user),
	UserOff(Res.drawable.icon_material_user_off, Res.drawable.icon_fluent_user_off, Res.drawable.icon_lucide_user_off),
}

internal enum class IconFamily {
	Material,
	Fluent,
	Lucide,
}

internal fun iconFamilyFor(uiPlatform: UiPlatform): IconFamily = when (uiPlatform) {
	UiPlatform.Android -> IconFamily.Material
	UiPlatform.Windows -> IconFamily.Fluent
	UiPlatform.Linux,
	UiPlatform.Desktop,
	-> IconFamily.Lucide
}

internal fun AppIcon.resourceFor(uiPlatform: UiPlatform): DrawableResource = when (iconFamilyFor(uiPlatform)) {
	IconFamily.Material -> material
	IconFamily.Fluent -> fluent
	IconFamily.Lucide -> lucide
}

@Composable
internal fun PlatformIcon(
	icon: AppIcon,
	contentDescription: String?,
	modifier: Modifier = Modifier,
	tint: Color = LocalContentColor.current,
) {
	Icon(
		painter = painterResource(icon.resourceFor(LocalUiPlatform.current)),
		contentDescription = contentDescription,
		modifier = modifier,
		tint = tint,
	)
}
