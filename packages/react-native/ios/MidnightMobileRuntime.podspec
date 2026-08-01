require 'json'

package = JSON.parse(File.read(File.join(__dir__, '..', 'package.json')))

Pod::Spec.new do |s|
  s.name             = 'MidnightMobileRuntime'
  s.version          = package['version']
  s.summary          = package['description']
  s.description      = package['description']
  s.author           = '1AM contributors'
  s.homepage         = 'https://github.com/ADGLx/midnight-mobile'
  s.license          = package['license']
  s.source           = { :git => 'https://github.com/ADGLx/midnight-mobile.git',
                         :tag => "v#{s.version}" }
  s.platforms        = { :ios => '15.1' }
  s.swift_version    = '5.9'
  s.module_name      = 'MidnightMobileExpo'

  s.dependency 'ExpoModulesCore'
  s.source_files = [
    'MidnightMobileRuntimeModule.swift',
    'MidnightMobileLocalProverModule.swift',
    'generated/MidnightMobileRuntime.swift'
  ]
  s.vendored_frameworks = 'build/MidnightMobileRuntime.xcframework'
  s.preserve_paths = [
    'MidnightMobileLocalProverFFI.h',
    'generated/*',
    'build/MidnightMobileRuntime.xcframework'
  ]
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_COMPILATION_MODE' => 'wholemodule'
  }
end
