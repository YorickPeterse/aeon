# frozen_string_literal: true

module Inkoc
  module AST
    class Send
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :receiver, :arguments, :type_arguments, :location

      attr_accessor :receiver_type, :block_type, :throw_type, :imported,
                    :create_iterator

      # name - The name of the message as a String.
      # receiver - The object to send the message to.
      # type_arguments - The type arguments to pass.
      # arguments - The arguments to pass.
      # location - The SourceLocation of the message send.
      def initialize(name, receiver, type_arguments, arguments, location)
        @name = name
        @receiver = receiver
        @type_arguments = type_arguments
        @arguments = arguments
        @location = location
        @receiver_type = nil
        @method_type = nil
        @throw_type = nil
        @create_iterator = false
      end

      def send?
        true
      end

      def visitor_method
        if raw_instruction?
          :on_raw_instruction
        else
          :on_send
        end
      end

      def raw_instruction?
        receiver&.constant? &&
          receiver&.name == Config::RAW_INSTRUCTION_RECEIVER
      end

      def raw_instruction_visitor_method
        :"on_raw_#{name}"
      end
    end
  end
end
