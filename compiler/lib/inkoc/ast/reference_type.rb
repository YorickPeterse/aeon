# frozen_string_literal: true

module Inkoc
  module AST
    class ReferenceType
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :type_node, :location
      attr_writer :optional

      def initialize(type, location)
        @type_node = type
        @location = location
      end

      def optional?
        @optional
      end

      def late_binding=(value)
        @type_node.late_binding = value
      end

      def late_binding?
        @type_node.late_binding?
      end

      def visitor_method
        :on_reference_type
      end
    end
  end
end
