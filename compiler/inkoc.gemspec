require File.expand_path('../lib/inkoc/version', __FILE__)

Gem::Specification.new do |gem|
  gem.name        = 'inkoc'
  gem.version     = Inkoc::VERSION
  gem.authors     = ['Yorick Peterse']
  gem.email       = 'yorickpeterse@gmail.com'
  gem.summary     = 'The Inko compiler'
  gem.homepage    = 'https://gitlab.com.com/inko-lang/inko/'
  gem.description = gem.summary
  gem.executables = %w[inkoc inko-test]
  gem.license     = 'MPL-2.0'

  gem.files = Dir.glob([
    'bin/inkoc',
    'bin/inko-test',
    'lib/**/*.rb',
    'LICENSE',
    'README.md'
  ]).select { |file| File.file?(file) }

  gem.required_ruby_version = '>= 2.4.0'

  gem.add_dependency 'ansi', '~> 1.5'
  gem.add_dependency 'sxdg', '~> 1.0'

  gem.add_development_dependency 'rake', '~> 12.0'
  gem.add_development_dependency 'rspec', '~> 3.6'
  gem.add_development_dependency 'rubocop', '~> 0.49'
  gem.add_development_dependency 'rubocop-rspec', '~> 1.15'
end
