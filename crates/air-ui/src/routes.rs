use air_ui::icons::Icon;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AppRoute {
    Dashboard,
    RulesProxy,
    OverrideScript,
    ProxyGroups,
    Connections,
    Subscriptions,
    Logs,
    Profiles,
    Settings,
}

#[derive(Clone, Copy, Debug)]
pub struct RouteDescriptor {
    pub route: AppRoute,
    pub label: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub icon: Icon,
}

impl AppRoute {
    pub const ALL: [AppRoute; 7] = [
        AppRoute::Dashboard,
        AppRoute::Subscriptions,
        AppRoute::ProxyGroups,
        AppRoute::Connections,
        AppRoute::RulesProxy,
        AppRoute::OverrideScript,
        AppRoute::Settings,
    ];

    /// 默认首屏。
    ///
    /// 首次启动应进入仪表盘，让用户一眼看到运行状态和内核启动按钮。
    pub const DEFAULT: AppRoute = AppRoute::Dashboard;

    pub fn all() -> &'static [AppRoute] {
        &Self::ALL
    }

    pub fn descriptor(self) -> RouteDescriptor {
        match self {
            AppRoute::Dashboard => RouteDescriptor {
                route: self,
                label: "仪表盘",
                title: "仪表盘",
                description: "总览网络速度、流量统计、系统代理与内核运行状态。",
                icon: Icon::Gauge,
            },
            AppRoute::RulesProxy => RouteDescriptor {
                route: self,
                label: "规则",
                title: "规则",
                description: "查看、过滤和临时启停 mihomo 当前运行态规则。",
                icon: Icon::ListFilter,
            },
            AppRoute::OverrideScript => RouteDescriptor {
                route: self,
                label: "覆写",
                title: "软件覆写",
                description: "编辑全局 JS 脚本，在写出运行配置前修改订阅合并结果。",
                icon: Icon::FilePenLine,
            },
            AppRoute::ProxyGroups => RouteDescriptor {
                route: self,
                label: "代理",
                title: "代理",
                description: "查看策略组成员、选择状态和健康检查结果。",
                icon: Icon::Layers,
            },
            AppRoute::Connections => RouteDescriptor {
                route: self,
                label: "连接",
                title: "连接管理",
                description: "查看、筛选和关闭 mihomo 当前连接。",
                icon: Icon::Cable,
            },
            AppRoute::Subscriptions => RouteDescriptor {
                route: self,
                label: "订阅",
                title: "订阅源",
                description: "管理订阅元数据、缓存和更新结果。",
                icon: Icon::Radio,
            },
            AppRoute::Logs => RouteDescriptor {
                route: self,
                label: "日志",
                title: "内核日志",
                description: "查看并筛选本地 core.log，页面每秒刷新一次。",
                icon: Icon::ScrollText,
            },
            AppRoute::Profiles => RouteDescriptor {
                route: self,
                label: "配置",
                title: "配置 Profile",
                description: "切换、编辑和校验 mihomo 配置 profile。",
                icon: Icon::FileSliders,
            },
            AppRoute::Settings => RouteDescriptor {
                route: self,
                label: "设置",
                title: "应用设置",
                description: "管理主题、窗口和平台集成选项。",
                icon: Icon::Settings2,
            },
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            AppRoute::Dashboard => "dashboard",
            AppRoute::RulesProxy => "rules-proxy",
            AppRoute::OverrideScript => "override-script",
            AppRoute::ProxyGroups => "proxy-groups",
            AppRoute::Connections => "connections",
            AppRoute::Subscriptions => "subscriptions",
            AppRoute::Logs => "logs",
            AppRoute::Profiles => "profiles",
            AppRoute::Settings => "settings",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppRoute;

    #[test]
    fn route_ids_are_stable_and_unique() {
        let mut ids = AppRoute::all()
            .iter()
            .map(|route| route.id())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), AppRoute::all().len());
        assert!(ids.contains(&"dashboard"));
        assert!(ids.contains(&"subscriptions"));
        assert!(ids.contains(&"rules-proxy"));
        assert!(ids.contains(&"override-script"));
        assert!(!ids.contains(&"proxies"));
        assert!(!ids.contains(&"rules"));
        assert!(!ids.contains(&"profiles"));
        assert!(!ids.contains(&"logs"));
        assert!(ids.contains(&"settings"));
    }

    #[test]
    fn route_menu_order_matches_title_bar_design() {
        let ids = AppRoute::all()
            .iter()
            .map(|route| route.id())
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "dashboard",
                "subscriptions",
                "proxy-groups",
                "connections",
                "rules-proxy",
                "override-script",
                "settings",
            ]
        );
    }

    #[test]
    fn every_visible_route_has_display_metadata() {
        for route in AppRoute::all() {
            let descriptor = route.descriptor();
            assert_eq!(descriptor.route, *route);
            assert!(!descriptor.label.is_empty());
            assert!(!descriptor.title.is_empty());
            assert!(!descriptor.description.is_empty());
            assert!(descriptor.icon.asset_path().ends_with(".svg"));
        }
    }

    #[test]
    fn dashboard_is_the_first_route_and_default_landing_page() {
        // 需求要求仪表盘作为第一页面，且首次启动落在仪表盘。
        assert_eq!(AppRoute::all().first(), Some(&AppRoute::Dashboard));
        assert_eq!(AppRoute::DEFAULT, AppRoute::Dashboard);
        assert_eq!(AppRoute::DEFAULT.id(), "dashboard");
    }

    #[test]
    fn hidden_log_route_has_display_metadata() {
        let descriptor = AppRoute::Logs.descriptor();
        assert_eq!(descriptor.route, AppRoute::Logs);
        assert_eq!(AppRoute::Logs.id(), "logs");
        assert!(!AppRoute::all().contains(&AppRoute::Logs));
    }
}
