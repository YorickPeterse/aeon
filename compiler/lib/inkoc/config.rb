# frozen_string_literal: true

module Inkoc
  class Config
    PROGRAM_NAME = 'inkoc'

    # The name of the directory to store bytecode files in.
    BYTECODE_DIR = 'bytecode'

    # The file extension of bytecode files.
    BYTECODE_EXT = '.inkoc'

    # The file extension of source files.
    SOURCE_EXT = '.inko'

    # The name of the root module for the standard library.
    STD_MODULE = 'std'

    # The path to the bootstrap module.
    BOOTSTRAP_MODULE = 'bootstrap'

    # The path to the prelude module.
    PRELUDE_MODULE = 'prelude'

    OBJECT_BUILTIN = '_Object'
    TRAIT_BUILTIN = '_Trait'
    ARRAY_BUILTIN = '_Array'
    HASH_MAP_BUILTIN = '_HashMap'

    # The name of the constant to use as the receiver for raw instructions.
    RAW_INSTRUCTION_RECEIVER = '__INKOC'

    NEW_MESSAGE = 'new'
    DEFINE_REQUIRED_METHOD_MESSAGE = 'define_required_method'
    CALL_MESSAGE = 'call'
    SELF_LOCAL = 'self'
    LOAD_MODULE_MESSAGE = 'load_module'
    SYMBOL_MESSAGE = 'symbol'
    DEFINE_MODULE_MESSAGE = 'define_module'
    MODULE_SEPARATOR = '::'

    attr_reader :source_directories, :mode, :target

    def initialize
      @source_directories = []
      @mode = :debug
      @target = File.join(SXDG::XDG_CACHE_HOME, PROGRAM_NAME, BYTECODE_DIR)
    end

    def target=(path)
      @target = File.expand_path(path)
    end

    def release_mode?
      @mode == :release
    end

    def release_mode
      @mode = :release
    end

    def add_source_directories(directories)
      @source_directories |= directories
    end

    def create_directories
      FileUtils.mkdir_p(@target)
    end
  end
end