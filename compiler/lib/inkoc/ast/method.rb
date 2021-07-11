# frozen_string_literal: true

module Inkoc
  module AST
    class Method
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :arguments, :type_parameters, :throws, :method_bounds,
                  :body, :location, :yields

      attr_accessor :static, :returns, :move

      # name - The name of the method.
      # args - The arguments of the method.
      # returns - The return type of the method.
      # yields - The yield type of the method.
      # throws - The type being thrown by this method.
      # required - If the method is a required method in a trait.
      # method_bounds - Additional type requirements for this method.
      # body - The body of the method.
      # loc - The SourceLocation of this method.
      def initialize(
        name,
        args,
        targs,
        returns,
        yields,
        throws,
        required,
        method_bounds,
        body,
        loc
      )
        @name = name
        @arguments = args
        @type_parameters = targs
        @returns = returns
        @yields = yields
        @throws = throws
        @method_bounds = method_bounds
        @body = body
        @location = loc
        @required = required
        @type = nil
        @static = false
        @explicit_return_type = !returns.nil?
        @move = false
      end

      def required?
        @required
      end

      def explicit_return_type?
        @explicit_return_type
      end

      def visitor_method
        required? ? :on_required_method : :on_method
      end

      def method?
        true
      end

      def block_type
        type
      end
    end
  end
end
