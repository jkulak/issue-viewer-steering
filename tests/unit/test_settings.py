from pathlib import Path

import pytest

from issue_viewer.mounts import compose_override
from issue_viewer.settings import ConfigError, load_settings, parse_projects


def test_projects_are_parsed_with_the_read_only_suffix():
    projects = parse_projects(" notes=/home/u/notes/tasks , archive=/srv/old/tasks/:ro,x=/y:rw")
    assert [(p.name, p.path, p.read_only) for p in projects] == [
        ("notes", Path("/home/u/notes/tasks"), False),
        ("archive", Path("/srv/old/tasks"), True),
        ("x", Path("/y"), False),
    ]


@pytest.mark.parametrize(
    "value",
    [
        "notes",
        "notes=",
        "=/a",
        "notes=relative/path",
        "static=/a",
        "bad name=/a",
        "a=/x,a=/y",
        "../up=/a",
    ],
)
def test_entries_that_are_not_a_name_and_an_absolute_path_are_refused(value):
    with pytest.raises(ConfigError):
        parse_projects(value)


def test_an_empty_project_list_is_allowed():
    assert parse_projects("") == ()


def test_loopback_hosts_are_always_allowed_and_extra_ones_are_added():
    settings = load_settings({"ALLOWED_HOSTS": "Viewer.local, e2e-app"})
    assert settings.allowed_hosts == ("localhost", "127.0.0.1", "viewer.local", "e2e-app")


def test_settings_come_from_the_environment():
    settings = load_settings(
        {"PROJECTS": "a=/a", "TZ": "Europe/Warsaw", "REFRESH_SECONDS": "0", "LOG_LEVEL": "debug"}
    )
    assert settings.projects[0].name == "a"
    assert str(settings.timezone) == "Europe/Warsaw"
    assert settings.refresh_seconds == 0
    assert settings.log_level == "DEBUG"


@pytest.mark.parametrize(
    "env", [{"TZ": "Mars/Olympus"}, {"REFRESH_SECONDS": "-1"}, {"LISTEN_PORT": "x"}]
)
def test_bad_values_are_configuration_errors(env):
    with pytest.raises(ConfigError):
        load_settings(env)


def test_the_compose_override_mounts_each_directory_at_its_own_path():
    override = compose_override(parse_projects("a=/home/u/a/tasks,b=/srv/b:ro"))
    assert override == {
        "services": {
            "app": {
                "volumes": [
                    {
                        "type": "bind",
                        "source": "/home/u/a/tasks",
                        "target": "/home/u/a/tasks",
                        "read_only": False,
                    },
                    {"type": "bind", "source": "/srv/b", "target": "/srv/b", "read_only": True},
                ]
            }
        }
    }
