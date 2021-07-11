# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Database
      attr_reader :true_type, :false_type, :nil_type, :block_type,
                  :integer_type, :float_type, :string_type, :array_type,
                  :trait_type, :boolean_type, :byte_array_type,
                  :module_type, :ffi_library_type, :ffi_function_type,
                  :ffi_pointer_type, :ip_socket_type, :unix_socket_type,
                  :process_type, :read_only_file_type, :write_only_file_type,
                  :read_write_file_type, :hasher_type, :generator_type,
                  :child_process_type, :unsigned_integer_type

      def initialize
        @trait_type = new_builtin_object(Config::TRAIT_CONST)
        @boolean_type = new_builtin_object(Config::BOOLEAN_CONST)
        @true_type = @boolean_type.new_instance
        @false_type = @boolean_type.new_instance
        @nil_type = new_builtin_object(Config::NIL_CONST)
        @block_type = new_builtin_object(Config::BLOCK_CONST)
        @integer_type = new_builtin_object(Config::INTEGER_CONST)
        @float_type = new_builtin_object(Config::FLOAT_CONST)
        @string_type = new_builtin_object(Config::STRING_CONST)
        @byte_array_type = new_builtin_object(Config::BYTE_ARRAY_CONST)
        @array_type = initialize_array_type
        @module_type = new_builtin_object(Config::MODULE_TYPE)
        @ffi_library_type = new_builtin_object(Config::FFI_LIBRARY_TYPE)
        @ffi_function_type = new_builtin_object(Config::FFI_FUNCTION_TYPE)
        @ffi_pointer_type = new_builtin_object(Config::FFI_POINTER_TYPE)
        @ip_socket_type = new_builtin_object(Config::IP_SOCKET_TYPE)
        @unix_socket_type = new_builtin_object(Config::UNIX_SOCKET_TYPE)
        @process_type = new_builtin_object(Config::PROCESS_TYPE)
        @read_only_file_type = new_builtin_object(Config::READ_ONLY_FILE_TYPE)
        @write_only_file_type = new_builtin_object(Config::WRITE_ONLY_FILE_TYPE)
        @read_write_file_type = new_builtin_object(Config::READ_WRITE_FILE_TYPE)
        @hasher_type = new_builtin_object(Config::HASHER_TYPE)
        @generator_type = initialize_generator_type
        @child_process_type = new_builtin_object(Config::CHILD_PROCESS_TYPE)
        @unsigned_integer_type = new_builtin_object(Config::UNSIGNED_INTEGER_TYPE)
        @trait_id = -1
      end

      def new_array_of_type(type)
        array_type.new_instance([type])
      end

      def new_builtin_object(name)
        Object.new(name: name, builtin: true)
      end

      def new_object_type(name)
        Object.new(name: name)
      end

      def new_empty_object
        Object.new
      end

      def new_trait_type(name, proto = trait_type)
        Trait.new(name: name, prototype: proto, unique_id: @trait_id += 1)
      end

      def initialize_array_type
        new_builtin_object(Config::ARRAY_CONST).tap do |array|
          array.define_type_parameter(Config::ARRAY_TYPE_PARAMETER)
        end
      end

      def initialize_generator_type
        new_builtin_object(Config::GENERATOR_TYPE).tap do |gen|
          gen.define_type_parameter(Config::GENERATOR_YIELD_TYPE_PARAMETER)
          gen.define_type_parameter(Config::GENERATOR_THROW_TYPE_PARAMETER)
        end
      end
    end
  end
end
