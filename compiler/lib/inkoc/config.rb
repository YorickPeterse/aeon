# frozen_string_literal: true

module Inkoc
  class Config
    CACHE_NAME = 'inko'

    # The name of the directory to store bytecode files in.
    BYTECODE_DIR = 'bytecode'

    # The file extension of bytecode files.
    BYTECODE_EXT = '.ibi'

    # The file extension of source files.
    SOURCE_EXT = '.inko'

    # The name of the root module for the standard library.
    STD_MODULE = 'std'

    # The path to the bootstrap module.
    BOOTSTRAP_MODULE = 'bootstrap'

    INIT_MODULE = 'init'
    OPERATORS_MODULE = 'std::operators'
    CONVERSION_MODULE = 'std::conversion'
    CLONE_MODULE = 'std::clone'
    PAIR_MODULE = 'std::pair'
    ITER_MODULE = 'std::iterator'

    OBJECT_CONST = 'Object'
    OPTION_CONST = 'Option'
    TRAIT_CONST = 'Trait'
    ARRAY_CONST = 'Array'
    BLOCK_CONST = 'Block'
    INTEGER_CONST = 'Int'
    UNSIGNED_INTEGER_TYPE = 'UnsignedInt'
    FLOAT_CONST = 'Float'
    STRING_CONST = 'String'
    TRUE_CONST = 'True'
    FALSE_CONST = 'False'
    BOOLEAN_CONST = 'Boolean'
    NIL_CONST = 'Nil'
    FILE_CONST = 'File'
    BYTE_ARRAY_CONST = 'ByteArray'
    ARRAY_TYPE_PARAMETER = 'T'
    OPTIONAL_CONST = 'Optional'
    MATCH_CONST = 'Match'
    TO_STRING_CONST = 'ToString'
    COPY_CONST = 'Copy'

    MODULE_TYPE = 'Module'
    FFI_LIBRARY_TYPE = 'Library'
    FFI_FUNCTION_TYPE = 'Function'
    FFI_POINTER_TYPE = 'Pointer'
    IP_SOCKET_TYPE = 'Socket'
    UNIX_SOCKET_TYPE = 'Socket'
    PROCESS_TYPE = 'Process'
    READ_ONLY_FILE_TYPE = 'ReadOnlyFile'
    WRITE_ONLY_FILE_TYPE = 'WriteOnlyFile'
    READ_WRITE_FILE_TYPE = 'ReadWriteFile'
    HASHER_TYPE = 'Hasher'
    GENERATOR_TYPE = 'Generator'
    CHILD_PROCESS_TYPE = 'ChildProcess'
    SELF_TYPE = 'Self'
    NEVER_TYPE = 'Never'
    ANY_TYPE = 'Any'
    MODULES_ATTRIBUTE = 'Modules'
    PAIR_TYPE = 'Pair'
    TRIPLE_TYPE = 'Triple'
    ITER_TYPE = 'Iterator'

    # The name of the constant to use as the receiver for raw instructions.
    RAW_INSTRUCTION_RECEIVER = '_INKOC'
    NEW_MESSAGE = 'new'
    SET_INDEX_MESSAGE = '[]='
    MODULE_GLOBAL = 'ThisModule'
    CALL_MESSAGE = 'call'
    PANIC_MESSAGE = 'panic'
    TO_STRING_MESSAGE = 'to_string'
    MODULE_SEPARATOR = '::'
    BLOCK_TYPE_NAME = 'do'
    LAMBDA_TYPE_NAME = 'lambda'
    BLOCK_NAME = '<block>'
    LAMBDA_NAME = '<lambda>'
    TRY_BLOCK_NAME = '<try>'
    ELSE_BLOCK_NAME = '<else>'
    IMPL_NAME = '<impl>'
    OBJECT_NAME_INSTANCE_ATTRIBUTE = '@_object_name'
    IMPLEMENTED_TRAITS_INSTANCE_ATTRIBUTE = '@_implemented_traits'
    INIT_MESSAGE = 'init'
    MATCH_MESSAGE = '=~'
    TRUTHY_MESSAGE = 'truthy?'
    ITER_MESSAGE = 'iter'
    NEXT_MESSAGE = 'next'
    GET_MESSAGE = 'get'

    GENERATOR_YIELD_TYPE_PARAMETER = 'T'
    GENERATOR_THROW_TYPE_PARAMETER = 'E'

    RESERVED_CONSTANTS = Set.new(
      [
        MODULE_GLOBAL,
        RAW_INSTRUCTION_RECEIVER,
        SELF_TYPE,
        NEVER_TYPE,
      ]
    ).freeze

    DEFAULT_RUNTIME_PATH = '/usr/lib/inko/runtime'

    MAXIMUM_METHOD_ARGUMENTS = 255

    PRELUDE_SYMBOLS = {
      'option' => %w[Option],
      'map' => %w[Map],
      'range' => %w[Range],
    }.freeze

    attr_reader :source_directories

    def self.std_module_name(name)
      "#{STD_MODULE}#{MODULE_SEPARATOR}#{name}"
    end

    def initialize(compile: true)
      @source_directories = Set.new([runtime_directory])
      @compile = compile
    end

    def compile?
      @compile
    end

    def add_source_directories(directories)
      directories.each do |dir|
        @source_directories << Pathname.new(File.expand_path(dir))
      end
    end

    def runtime_directory
      dir = ENV['INKO_RUNTIME_PATH']

      if dir.nil? || dir.empty?
        dir = DEFAULT_RUNTIME_PATH
      end

      Pathname.new(dir)
    end
  end
end
