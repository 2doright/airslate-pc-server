#[cfg(windows)]
pub(crate) mod windows_app {
    use std::{
        fmt, mem,
        path::{Path, PathBuf},
    };

    use windows::{
        Win32::{
            Devices::{
                DeviceAndDriverInstallation::{
                    CM_Get_DevNode_Status, CM_PROB_FAILED_INSTALL, CR_SUCCESS, DI_ENUMSINGLEINF,
                    DI_FLAGSEX_ALLOWEXCLUDEDDRVS, DICS_FLAG_GLOBAL, DIGCF_ALLCLASSES,
                    DIGCF_PRESENT, DIIDFLAG_NOFINISHINSTALLUI, DIREG_DEV, DiInstallDevice,
                    HDEVINFO, SETUP_DI_DEVICE_INSTALL_FLAGS, SETUP_DI_DEVICE_INSTALL_FLAGS_EX,
                    SP_DEVINFO_DATA, SP_DEVINSTALL_PARAMS_W, SP_DRVINFO_DATA_V2_W,
                    SP_DRVINFO_DETAIL_DATA_W, SPDIT_CLASSDRIVER, SetupDiBuildDriverInfoList,
                    SetupDiCreateDeviceInfoList, SetupDiDestroyDeviceInfoList,
                    SetupDiDestroyDriverInfoList, SetupDiEnumDeviceInfo, SetupDiEnumDriverInfoW,
                    SetupDiGetClassDevsW, SetupDiGetDeviceInstallParamsW,
                    SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
                    SetupDiGetDriverInfoDetailW, SetupDiGetINFClassW, SetupDiOpenDevRegKey,
                    SetupDiOpenDeviceInfoW, SetupDiSetDeviceInstallParamsW,
                    SetupDiSetSelectedDriverW,
                },
                Properties::{
                    DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_LocationPaths,
                    DEVPKEY_Device_Service, DEVPROP_TYPE_STRING, DEVPROP_TYPE_STRING_LIST,
                    DEVPROPTYPE,
                },
            },
            Foundation::{
                DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, ERROR_NOT_FOUND,
                ERROR_SUCCESS,
            },
            System::{
                Registry::{KEY_SET_VALUE, REG_MULTI_SZ, RegCloseKey, RegSetValueExW},
                SystemInformation::GetSystemWindowsDirectoryW,
            },
        },
        core::{BOOL, Error as WindowsError, GUID, HRESULT, PCWSTR},
    };

    const USBDEVICE_CLASS_GUID: GUID = GUID::from_u128(0x88bae032_5a81_49f0_bc3d_a4ff138216d6);
    const AIRSLATE_USB_INTERFACE_GUID: GUID =
        GUID::from_u128(0x9658c676_474f_4d5e_bfcb_3f7747eb5dd8);
    const ACCESSORY_COMPATIBLE_ID: &str = "USB\\Class_FF&SubClass_FF&Prot_00";

    #[allow(dead_code)]
    pub(crate) fn find_present_usb_location(
        vid: u16,
        pid: u16,
        serial: Option<&str>,
    ) -> Result<String, String> {
        find_present_usb_location_inner(vid, pid, serial).map_err(|error| error.to_string())
    }

    #[allow(dead_code)]
    pub(crate) fn code28_instance_at_location(
        location_path: &str,
    ) -> Result<Option<String>, String> {
        let candidates =
            enumerate_candidates(Some(location_path)).map_err(|error| error.to_string())?;
        match candidates.as_slice() {
            [] => Ok(None),
            [candidate] => Ok(Some(candidate.instance_id.clone())),
            candidates => Err(format!(
                "{} safe Code 28 accessory candidates share LocationPath {location_path:?}",
                candidates.len()
            )),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn install_confirmed_inbox_winusb(
        location_path: &str,
        instance_id: &str,
    ) -> Result<(), String> {
        install_inbox_winusb(location_path, instance_id).map_err(|error| error.to_string())
    }

    #[allow(dead_code)]
    fn find_present_usb_location_inner(
        vid: u16,
        pid: u16,
        serial: Option<&str>,
    ) -> Result<String, DryRunError> {
        let set = unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
        }
        .map_err(|error| DryRunError::Windows("enumerating present USB devices", error))?;
        let set = DeviceInfoSet(set);
        let prefix = format!("USB\\VID_{vid:04X}&PID_{pid:04X}\\");
        let mut locations = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            match unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut info) } {
                Ok(()) => {}
                Err(error) if is_win32_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
                Err(error) => return Err(DryRunError::Windows("enumerating USB devnode", error)),
            }
            index += 1;
            let instance_id = get_instance_id(set.0, &info)?;
            if !instance_id.to_ascii_uppercase().starts_with(&prefix)
                || serial.is_some_and(|serial| {
                    !instance_id
                        .rsplit('\\')
                        .next()
                        .is_some_and(|value| value.eq_ignore_ascii_case(serial))
                })
            {
                continue;
            }
            let paths = get_string_list_property(set.0, &info, &DEVPKEY_Device_LocationPaths)?
                .unwrap_or_default();
            if let Some(path) = paths.first() {
                locations.push(path.clone());
            }
        }
        match locations.as_slice() {
            [location] => Ok(location.clone()),
            [] => Err(DryRunError::MissingCandidate),
            locations => Err(DryRunError::AmbiguousCandidates(locations.len())),
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct CandidateFacts {
        instance_id: String,
        location_paths: Vec<String>,
        compatible_ids: Vec<String>,
        service: Option<String>,
        problem_code: u32,
    }

    impl CandidateFacts {
        fn is_safe_target(&self, expected_location: Option<&str>) -> bool {
            self.problem_code == CM_PROB_FAILED_INSTALL.0
                && self.service.as_deref().is_none_or(str::is_empty)
                && self
                    .compatible_ids
                    .iter()
                    .any(|id| id.eq_ignore_ascii_case(ACCESSORY_COMPATIBLE_ID))
                && expected_location.is_none_or(|expected| {
                    self.location_paths
                        .iter()
                        .any(|path| path.eq_ignore_ascii_case(expected))
                })
        }
    }

    fn enumerate_candidates(
        expected_location: Option<&str>,
    ) -> Result<Vec<CandidateFacts>, DryRunError> {
        let set = unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
        }
        .map_err(|error| DryRunError::Windows("enumerating present devices", error))?;
        let set = DeviceInfoSet(set);

        let mut candidates = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            match unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut info) } {
                Ok(()) => {}
                Err(error) if is_win32_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
                Err(error) => {
                    return Err(DryRunError::Windows("enumerating device element", error));
                }
            }
            index += 1;

            let mut status = Default::default();
            let mut problem = Default::default();
            let result =
                unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, info.DevInst, 0) };
            if result != CR_SUCCESS {
                continue;
            }

            let facts = CandidateFacts {
                instance_id: get_instance_id(set.0, &info)?,
                location_paths: get_string_list_property(
                    set.0,
                    &info,
                    &DEVPKEY_Device_LocationPaths,
                )?
                .unwrap_or_default(),
                compatible_ids: get_string_list_property(
                    set.0,
                    &info,
                    &DEVPKEY_Device_CompatibleIds,
                )?
                .unwrap_or_default(),
                service: get_string_property(set.0, &info, &DEVPKEY_Device_Service)?,
                problem_code: problem.0,
            };
            if facts.is_safe_target(expected_location) {
                candidates.push(facts);
            }
        }
        Ok(candidates)
    }

    fn install_inbox_winusb(
        expected_location: &str,
        confirmed_instance_id: &str,
    ) -> Result<(), DryRunError> {
        let set = unsafe { SetupDiCreateDeviceInfoList(None, None) }
            .map_err(|error| DryRunError::Windows("creating installation device set", error))?;
        let set = DeviceInfoSet(set);
        let mut target = SP_DEVINFO_DATA {
            cbSize: mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let instance_wide = null_terminated(Path::new(confirmed_instance_id));
        unsafe {
            SetupDiOpenDeviceInfoW(
                set.0,
                PCWSTR::from_raw(instance_wide.as_ptr()),
                None,
                0,
                Some(&mut target),
            )
        }
        .map_err(|error| DryRunError::Windows("opening confirmed target devnode", error))?;

        let facts = facts_for_info(set.0, &target)?;
        if !facts
            .instance_id
            .eq_ignore_ascii_case(confirmed_instance_id)
            || !facts.is_safe_target(Some(expected_location))
        {
            return Err(DryRunError::TargetChanged);
        }

        let expected_inf = system_winusb_inf_path()?;
        let search = InstallDriverSearch::new(expected_inf.clone());
        configure_device_driver_search(set.0, &target, &search)?;
        unsafe { SetupDiBuildDriverInfoList(set.0, Some(&mut target), SPDIT_CLASSDRIVER) }
            .map_err(|error| {
                DryRunError::Windows("building target-associated installation driver list", error)
            })?;
        let _drivers = DriverInfoList {
            set: set.0,
            device: Some(target),
            kind: SPDIT_CLASSDRIVER,
        };
        let mut matching = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DRVINFO_DATA_V2_W {
                cbSize: mem::size_of::<SP_DRVINFO_DATA_V2_W>() as u32,
                ..Default::default()
            };
            match unsafe {
                SetupDiEnumDriverInfoW(set.0, Some(&target), SPDIT_CLASSDRIVER, index, &mut info)
            } {
                Ok(()) => {}
                Err(error) if is_win32_error(&error, ERROR_NO_MORE_ITEMS.0) => break,
                Err(error) => {
                    return Err(DryRunError::Windows(
                        "enumerating installation driver node",
                        error,
                    ));
                }
            }
            index += 1;
            let detail = get_driver_detail(set.0, Some(&target), &info)?;
            if !is_generic_inbox_winusb_node(&detail.inf_path, &detail.section, &expected_inf)
                || !utf16_field(&info.ProviderName).eq_ignore_ascii_case("Microsoft")
                || inf_class_guid(&detail.inf_path)? != USBDEVICE_CLASS_GUID
            {
                continue;
            }
            matching.push(info);
        }
        let mut driver = match matching.as_slice() {
            [driver] => *driver,
            [] => return Err(DryRunError::MissingWinUsbNode),
            nodes => return Err(DryRunError::AmbiguousWinUsbNodes(nodes.len())),
        };

        println!(
            "selected target-associated driver: instance={:?} devinst={} driver_reserved={} inf={:?}",
            confirmed_instance_id, target.DevInst, driver.Reserved, expected_inf
        );
        unsafe { SetupDiSetSelectedDriverW(set.0, Some(&mut target), Some(&mut driver)) }.map_err(
            |error| {
                DryRunError::Windows("selecting target-associated Microsoft inbox WinUSB", error)
            },
        )?;

        let mut need_reboot = BOOL::from(false);
        unsafe {
            DiInstallDevice(
                None,
                set.0,
                &target,
                Some(&driver),
                DIIDFLAG_NOFINISHINSTALLUI,
                Some(&mut need_reboot),
            )
        }
        .map_err(|error| DryRunError::Windows("installing Microsoft inbox WinUSB", error))?;
        write_interface_guid(set.0, &target)?;
        println!(
            "installed Microsoft inbox WinUSB on confirmed instance {:?}; DeviceInterfaceGUIDs={:?}; need_reboot={}",
            confirmed_instance_id,
            AIRSLATE_USB_INTERFACE_GUID,
            need_reboot.as_bool()
        );
        println!(
            "disconnect/reconnect the same physical port before opening Bulk; if need_reboot=true, restart Windows first"
        );
        Ok(())
    }

    fn facts_for_info(
        set: HDEVINFO,
        info: &SP_DEVINFO_DATA,
    ) -> Result<CandidateFacts, DryRunError> {
        let mut status = Default::default();
        let mut problem = Default::default();
        let result = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, info.DevInst, 0) };
        if result != CR_SUCCESS {
            return Err(DryRunError::ConfigManager(result.0));
        }
        Ok(CandidateFacts {
            instance_id: get_instance_id(set, info)?,
            location_paths: get_string_list_property(set, info, &DEVPKEY_Device_LocationPaths)?
                .unwrap_or_default(),
            compatible_ids: get_string_list_property(set, info, &DEVPKEY_Device_CompatibleIds)?
                .unwrap_or_default(),
            service: get_string_property(set, info, &DEVPKEY_Device_Service)?,
            problem_code: problem.0,
        })
    }

    #[derive(Debug, PartialEq, Eq)]
    struct InstallDriverSearch {
        inf_path: PathBuf,
        flags: SETUP_DI_DEVICE_INSTALL_FLAGS,
        flags_ex: SETUP_DI_DEVICE_INSTALL_FLAGS_EX,
    }

    impl InstallDriverSearch {
        fn new(inf_path: PathBuf) -> Self {
            Self {
                inf_path,
                flags: DI_ENUMSINGLEINF,
                flags_ex: DI_FLAGSEX_ALLOWEXCLUDEDDRVS,
            }
        }
    }

    fn configure_device_driver_search(
        set: HDEVINFO,
        target: &SP_DEVINFO_DATA,
        search: &InstallDriverSearch,
    ) -> Result<(), DryRunError> {
        let mut install_params = SP_DEVINSTALL_PARAMS_W {
            cbSize: mem::size_of::<SP_DEVINSTALL_PARAMS_W>() as u32,
            ..Default::default()
        };
        unsafe { SetupDiGetDeviceInstallParamsW(set, Some(target), &mut install_params) }.map_err(
            |error| {
                DryRunError::Windows("reading target-associated driver search parameters", error)
            },
        )?;

        let path = null_terminated(&search.inf_path);
        if path.len() > install_params.DriverPath.len() {
            return Err(DryRunError::DriverPathTooLong(path.len()));
        }
        install_params.DriverPath[..path.len()].copy_from_slice(&path);
        install_params.Flags |= search.flags;
        install_params.FlagsEx |= search.flags_ex;
        unsafe { SetupDiSetDeviceInstallParamsW(set, Some(target), &install_params) }.map_err(
            |error| DryRunError::Windows("configuring target-associated single-INF search", error),
        )
    }

    fn inf_class_guid(path: &Path) -> Result<GUID, DryRunError> {
        let path_wide = null_terminated(path);
        let mut class_guid = GUID::zeroed();
        let mut class_name = [0_u16; 64];
        unsafe {
            SetupDiGetINFClassW(
                PCWSTR::from_raw(path_wide.as_ptr()),
                &mut class_guid,
                &mut class_name,
                None,
            )
        }
        .map_err(|error| DryRunError::Windows("reading inbox INF class", error))?;
        Ok(class_guid)
    }

    fn write_interface_guid(set: HDEVINFO, info: &SP_DEVINFO_DATA) -> Result<(), DryRunError> {
        let key = unsafe {
            SetupDiOpenDevRegKey(set, info, DICS_FLAG_GLOBAL.0, 0, DIREG_DEV, KEY_SET_VALUE.0)
        }
        .map_err(|error| DryRunError::Windows("opening target device hardware key", error))?;
        let key = RegistryKey(key);
        let value_name = "DeviceInterfaceGUIDs\0".encode_utf16().collect::<Vec<_>>();
        let guid = format!("{{{:?}}}", AIRSLATE_USB_INTERFACE_GUID);
        let words = guid.encode_utf16().chain([0, 0]).collect::<Vec<_>>();
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let result = unsafe {
            RegSetValueExW(
                key.0,
                PCWSTR::from_raw(value_name.as_ptr()),
                None,
                REG_MULTI_SZ,
                Some(&bytes),
            )
        };
        if result != ERROR_SUCCESS {
            return Err(DryRunError::Registry(result.0));
        }
        Ok(())
    }

    struct DriverDetail {
        inf_path: PathBuf,
        section: String,
    }

    fn get_driver_detail(
        set: HDEVINFO,
        device: Option<&SP_DEVINFO_DATA>,
        driver: &SP_DRVINFO_DATA_V2_W,
    ) -> Result<DriverDetail, DryRunError> {
        let device = device.map(std::ptr::from_ref);
        let mut required = 0;
        let first = unsafe {
            SetupDiGetDriverInfoDetailW(set, device, driver, None, 0, Some(&mut required))
        };
        if let Err(error) = first
            && !is_win32_error(&error, ERROR_INSUFFICIENT_BUFFER.0)
        {
            return Err(DryRunError::Windows("sizing driver node detail", error));
        }

        let word_count = (required as usize).div_ceil(mem::size_of::<usize>());
        let mut storage = vec![0_usize; word_count.max(1)];
        let detail = storage.as_mut_ptr().cast::<SP_DRVINFO_DETAIL_DATA_W>();
        unsafe {
            (*detail).cbSize = mem::size_of::<SP_DRVINFO_DETAIL_DATA_W>() as u32;
            SetupDiGetDriverInfoDetailW(set, device, driver, Some(detail), required, None)
        }
        .map_err(|error| DryRunError::Windows("reading driver node detail", error))?;

        Ok(DriverDetail {
            inf_path: PathBuf::from(utf16_field(unsafe { &(*detail).InfFileName })),
            section: utf16_field(unsafe { &(*detail).SectionName }),
        })
    }

    fn get_instance_id(set: HDEVINFO, info: &SP_DEVINFO_DATA) -> Result<String, DryRunError> {
        let mut required = 0;
        let first = unsafe { SetupDiGetDeviceInstanceIdW(set, info, None, Some(&mut required)) };
        if let Err(error) = first
            && !is_win32_error(&error, ERROR_INSUFFICIENT_BUFFER.0)
        {
            return Err(DryRunError::Windows("sizing device instance ID", error));
        }
        let mut buffer = vec![0_u16; required as usize];
        unsafe { SetupDiGetDeviceInstanceIdW(set, info, Some(&mut buffer), None) }
            .map_err(|error| DryRunError::Windows("reading device instance ID", error))?;
        Ok(utf16_field(&buffer))
    }

    fn get_string_property(
        set: HDEVINFO,
        info: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Result<Option<String>, DryRunError> {
        let Some((kind, bytes)) = get_property(set, info, key)? else {
            return Ok(None);
        };
        if kind != DEVPROP_TYPE_STRING {
            return Err(DryRunError::UnexpectedPropertyType);
        }
        Ok(parse_utf16_multisz(&bytes).into_iter().next())
    }

    fn get_string_list_property(
        set: HDEVINFO,
        info: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Result<Option<Vec<String>>, DryRunError> {
        let Some((kind, bytes)) = get_property(set, info, key)? else {
            return Ok(None);
        };
        if kind != DEVPROP_TYPE_STRING_LIST {
            return Err(DryRunError::UnexpectedPropertyType);
        }
        Ok(Some(parse_utf16_multisz(&bytes)))
    }

    fn get_property(
        set: HDEVINFO,
        info: &SP_DEVINFO_DATA,
        key: &DEVPROPKEY,
    ) -> Result<Option<(DEVPROPTYPE, Vec<u8>)>, DryRunError> {
        let mut kind = DEVPROPTYPE::default();
        let mut required = 0;
        let first = unsafe {
            SetupDiGetDevicePropertyW(set, info, key, &mut kind, None, Some(&mut required), 0)
        };
        if let Err(error) = first {
            if is_win32_error(&error, ERROR_NOT_FOUND.0) {
                return Ok(None);
            }
            if !is_win32_error(&error, ERROR_INSUFFICIENT_BUFFER.0) {
                return Err(DryRunError::Windows("sizing device property", error));
            }
        }
        let mut bytes = vec![0_u8; required as usize];
        unsafe { SetupDiGetDevicePropertyW(set, info, key, &mut kind, Some(&mut bytes), None, 0) }
            .map_err(|error| DryRunError::Windows("reading device property", error))?;
        Ok(Some((kind, bytes)))
    }

    fn parse_utf16_multisz(bytes: &[u8]) -> Vec<String> {
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        words
            .split(|word| *word == 0)
            .filter(|part| !part.is_empty())
            .map(String::from_utf16_lossy)
            .collect()
    }

    fn utf16_field(field: &[u16]) -> String {
        let length = field
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(field.len());
        String::from_utf16_lossy(&field[..length])
    }

    fn null_terminated(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn system_winusb_inf_path() -> Result<PathBuf, DryRunError> {
        let mut buffer = vec![0_u16; 32_768];
        let length = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
        if length == 0 {
            return Err(DryRunError::Windows(
                "reading the system Windows directory",
                WindowsError::from_thread(),
            ));
        }
        if length >= buffer.len() {
            return Err(DryRunError::UnexpectedWindowsDirectoryLength(length));
        }
        let windows_directory = PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
        Ok(windows_directory.join("INF").join("winusb.inf"))
    }

    fn is_generic_inbox_winusb_node(path: &Path, section: &str, expected_inf: &Path) -> bool {
        path.to_string_lossy()
            .eq_ignore_ascii_case(&expected_inf.to_string_lossy())
            && section.eq_ignore_ascii_case("WINUSB")
    }

    fn is_win32_error(error: &WindowsError, code: u32) -> bool {
        error.code() == HRESULT::from_win32(code)
    }

    struct DeviceInfoSet(HDEVINFO);

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
        }
    }

    struct DriverInfoList {
        set: HDEVINFO,
        device: Option<SP_DEVINFO_DATA>,
        kind: windows::Win32::Devices::DeviceAndDriverInstallation::SETUP_DI_DRIVER_TYPE,
    }

    impl Drop for DriverInfoList {
        fn drop(&mut self) {
            let device = self.device.as_ref().map(std::ptr::from_ref);
            let _ = unsafe { SetupDiDestroyDriverInfoList(self.set, device, self.kind) };
        }
    }

    struct RegistryKey(windows::Win32::System::Registry::HKEY);

    impl Drop for RegistryKey {
        fn drop(&mut self) {
            let _ = unsafe { RegCloseKey(self.0) };
        }
    }

    #[derive(Debug)]
    enum DryRunError {
        Windows(&'static str, WindowsError),
        ConfigManager(u32),
        Registry(u32),
        UnexpectedWindowsDirectoryLength(usize),
        DriverPathTooLong(usize),
        UnexpectedPropertyType,
        TargetChanged,
        MissingWinUsbNode,
        AmbiguousCandidates(usize),
        AmbiguousWinUsbNodes(usize),
    }

    impl fmt::Display for DryRunError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Windows(operation, error) => {
                    write!(formatter, "failed while {operation}: {error}")
                }
                Self::ConfigManager(code) => {
                    write!(
                        formatter,
                        "Configuration Manager failed with CR code {code}"
                    )
                }
                Self::Registry(code) => {
                    write!(formatter, "registry write failed with Win32 code {code}")
                }
                Self::UnexpectedWindowsDirectoryLength(length) => write!(
                    formatter,
                    "GetSystemWindowsDirectoryW returned invalid length {length}"
                ),
                Self::DriverPathTooLong(length) => write!(
                    formatter,
                    "trusted winusb.inf path requires {length} UTF-16 words, exceeding SP_DEVINSTALL_PARAMS_W.DriverPath"
                ),
                Self::UnexpectedPropertyType => {
                    write!(
                        formatter,
                        "Windows returned an unexpected PnP property type"
                    )
                }
                Self::TargetChanged => write!(
                    formatter,
                    "the confirmed devnode facts changed before installation; refusing"
                ),
                Self::MissingWinUsbNode => write!(
                    formatter,
                    "no unique Microsoft inbox winusb.inf WINUSB USBDevice driver node exists"
                ),
                Self::AmbiguousCandidates(count) => write!(
                    formatter,
                    "{count} devices satisfy the strict Code28/FF-FF-00 guard; refusing ambiguity"
                ),
                Self::AmbiguousWinUsbNodes(count) => write!(
                    formatter,
                    "{count} inbox winusb.inf USBDevice driver nodes were found; refusing ambiguity"
                ),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn candidate() -> CandidateFacts {
            CandidateFacts {
                instance_id: "USB\\DYNAMIC\\SERIAL".into(),
                location_paths: vec!["PCIROOT(0)#USBROOT(0)#USB(2)".into()],
                compatible_ids: vec![ACCESSORY_COMPATIBLE_ID.into()],
                service: None,
                problem_code: 28,
            }
        }

        #[test]
        fn safety_guard_requires_every_runtime_fact() {
            assert!(candidate().is_safe_target(Some("PCIROOT(0)#USBROOT(0)#USB(2)")));

            let mut wrong_problem = candidate();
            wrong_problem.problem_code = 0;
            assert!(!wrong_problem.is_safe_target(None));

            let mut claimed = candidate();
            claimed.service = Some("WINUSB".into());
            assert!(!claimed.is_safe_target(None));

            let mut wrong_interface = candidate();
            wrong_interface.compatible_ids = vec!["USB\\Class_FF&SubClass_50&Prot_01".into()];
            assert!(!wrong_interface.is_safe_target(None));

            assert!(!candidate().is_safe_target(Some("PCIROOT(0)#USBROOT(0)#USB(3)")));
        }

        #[test]
        fn multisz_parser_discards_terminators() {
            let words = "first\0second\0\0".encode_utf16().collect::<Vec<_>>();
            let bytes = words
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>();
            assert_eq!(parse_utf16_multisz(&bytes), ["first", "second"]);
        }

        #[test]
        fn inbox_inf_check_requires_exact_system_path_and_is_case_insensitive() {
            let expected = Path::new(r"C:\Windows\INF\winusb.inf");
            assert!(is_generic_inbox_winusb_node(
                Path::new(r"c:\WINDOWS\inf\WINUSB.INF"),
                "winusb",
                expected
            ));
            assert!(!is_generic_inbox_winusb_node(
                Path::new(r"C:\Temp\winusb.inf"),
                "WINUSB",
                expected
            ));
            assert!(!is_generic_inbox_winusb_node(
                Path::new(r"C:\Windows\INF\winusb.inf"),
                "ADB",
                expected
            ));
        }

        #[test]
        fn install_driver_search_uses_exact_single_inf_flags() {
            let search = InstallDriverSearch::new(PathBuf::from(r"C:\Windows\INF\winusb.inf"));
            assert_eq!(search.flags, DI_ENUMSINGLEINF);
            assert_eq!(search.flags_ex, DI_FLAGSEX_ALLOWEXCLUDEDDRVS);
            assert_eq!(search.inf_path, PathBuf::from(r"C:\Windows\INF\winusb.inf"));
        }
    }
}
