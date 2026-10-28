rootProject.name = "ferrite-api-impl"
include("test-plugin")
project(":test-plugin").projectDir = file("../test-plugin")
include("test-dependency")
project(":test-dependency").projectDir = file("../test-dependency")
